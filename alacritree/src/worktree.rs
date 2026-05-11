//! Create and delete git worktrees on a background thread, streaming progress
//! back to the UI via an `mpsc` channel.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug, Clone)]
pub enum Progress {
    Step(String),
    Done(Result<PathBuf, String>),
}

pub struct CreateRequest {
    pub project_root: PathBuf,
    pub default_branch: Option<String>,
    pub branch: String,
}

/// git-check-ref-format rules, abridged: no whitespace/control chars, no
/// `..`, `~`, `^`, `:`, `?`, `*`, `[`, `\`, `@{`; can't start with `-` or `.`,
/// or end with `.` or `.lock`.
pub fn validate_branch_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Branch name is empty.".into());
    }
    if name.starts_with('-') {
        return Err("Branch name cannot start with `-`.".into());
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err("Branch name cannot start or end with `.`.".into());
    }
    if name.ends_with(".lock") {
        return Err("Branch name cannot end with `.lock`.".into());
    }
    if name.contains("..") || name.contains("@{") {
        return Err("Branch name cannot contain `..` or `@{`.".into());
    }
    for c in name.chars() {
        if c.is_whitespace() {
            return Err("Branch name cannot contain whitespace.".into());
        }
        if (c as u32) < 0x20 || c == '\u{7f}' {
            return Err("Branch name contains a control character.".into());
        }
        if matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\') {
            return Err(format!("Branch name cannot contain `{c}`."));
        }
    }
    Ok(())
}

pub fn spawn_create(req: CreateRequest, ctx: egui::Context) -> Receiver<Progress> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = run_create(&req, &tx, &ctx);
        let _ = tx.send(Progress::Done(result));
        ctx.request_repaint();
    });
    rx
}

fn run_create(
    req: &CreateRequest,
    tx: &Sender<Progress>,
    ctx: &egui::Context,
) -> Result<PathBuf, String> {
    let send = |s: &str| {
        let _ = tx.send(Progress::Step(s.to_string()));
        ctx.request_repaint();
    };

    let base =
        req.default_branch.clone().ok_or_else(|| "no default branch detected".to_string())?;

    send("Syncing with remote…");
    if !has_remote(&req.project_root, "origin") {
        return Err("no `origin` remote configured".into());
    }

    send(&format!("Verifying base branch `{base}`"));
    let base_ref = verify_base_branch(&req.project_root, &base)?;

    send("Fetching latest changes…");
    run_git(&req.project_root, &["fetch", "origin", &base])?;

    send("Creating git worktree…");
    let target = pick_worktree_path(&req.project_root, &req.branch)?;
    run_git(
        &req.project_root,
        &[
            "worktree",
            "add",
            target.to_str().ok_or("invalid worktree path")?,
            "-b",
            &req.branch,
            &base_ref,
        ],
    )?;

    send("Copying LLM configurations…");
    let copied = copy_llm_configs(&req.project_root, &target);
    if copied > 0 {
        send(&format!("Copied {copied} LLM config item(s)"));
    }

    // Pre-flip Claude Code's BEL setting so the user doesn't have to
    // configure each worktree by hand.  Other keys in the file are preserved.
    if let Err(e) = enable_claude_terminal_bell(&target) {
        log::warn!("failed to write Claude bell config in {}: {e}", target.display());
    } else {
        send("Enabled Claude Code terminal bell");
    }

    Ok(target)
}

fn enable_claude_terminal_bell(worktree_root: &Path) -> std::io::Result<()> {
    let dir = worktree_root.join(".claude");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("settings.local.json");

    let mut value: serde_json::Value = match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e),
    };
    if !value.is_object() {
        value = serde_json::json!({});
    }
    value["preferredNotifChannel"] = serde_json::json!("terminal_bell");

    let pretty = serde_json::to_string_pretty(&value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, pretty)
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let msg = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
    Err(format!("git {}: {msg}", args.join(" ")))
}

fn has_remote(cwd: &Path, name: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["remote", "get-url", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn verify_base_branch(cwd: &Path, branch: &str) -> Result<String, String> {
    let remote = format!("origin/{branch}");
    if Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--verify", "--quiet", &remote])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(remote);
    }
    if Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--verify", "--quiet", branch])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(branch.to_string());
    }
    Err(format!("base branch `{branch}` not found locally or on origin"))
}

/// Worktrees live under `~/.alacritree/worktrees/<project>-<hash>/<branch>` so
/// they don't clutter the repo's parent directory and stay grouped per app.
/// The path hash disambiguates same-named repos in different locations.
fn pick_worktree_path(repo: &Path, branch: &str) -> Result<PathBuf, String> {
    let parent = project_worktree_dir(repo)?;
    std::fs::create_dir_all(&parent)
        .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    let safe_branch: String =
        branch.chars().map(|c| if c == '/' || c.is_whitespace() { '-' } else { c }).collect();
    let mut candidate = parent.join(&safe_branch);
    let mut suffix = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{safe_branch}-{suffix}"));
        suffix += 1;
    }
    Ok(candidate)
}

fn project_worktree_dir(repo: &Path) -> Result<PathBuf, String> {
    let home = home::home_dir().ok_or_else(|| "could not locate home directory".to_string())?;
    let canonical = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    let project_name = canonical
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "project".to_string());

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    let hash = hasher.finish() as u32;

    Ok(home.join(".alacritree").join("worktrees").join(format!("{project_name}-{hash:08x}")))
}

/// Filenames/dirs at the project root that look like AI assistant config.
const LLM_CONFIG_NAMES: &[&str] = &[
    "CLAUDE.md",
    "CLAUDE.local.md",
    ".claude",
    ".clauderc",
    "AGENTS.md",
    ".cursorrules",
    ".cursor",
    ".aider.conf.yml",
    ".aiderignore",
    ".copilot-instructions.md",
    ".github/copilot-instructions.md",
    ".windsurfrules",
    ".roomodes",
    ".roo",
    ".codeium",
    ".continue",
];

fn copy_llm_configs(src_root: &Path, dst_root: &Path) -> usize {
    let mut copied = 0;
    for name in LLM_CONFIG_NAMES {
        let src = src_root.join(name);
        if !src.exists() {
            continue;
        }
        let dst = dst_root.join(name);
        if dst.exists() {
            continue;
        }
        match copy_path(&src, &dst) {
            Ok(()) => copied += 1,
            Err(e) => log::warn!("failed to copy {}: {e}", src.display()),
        }
    }
    copied
}

fn copy_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let child_dst = dst.join(entry.file_name());
            copy_path(&entry.path(), &child_dst)?;
        }
        Ok(())
    } else if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst).map(|_| ())
    } else {
        Ok(())
    }
}

pub fn delete_worktree(
    project_root: &Path,
    worktree_path: &Path,
    branch: Option<&str>,
    force: bool,
) -> Result<(), String> {
    let path_str = worktree_path.to_str().ok_or_else(|| "invalid worktree path".to_string())?;
    let mut args: Vec<&str> = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path_str);
    run_git(project_root, &args)?;
    if let Some(branch) = branch {
        // Branch may already be gone (e.g. detached HEAD) — ignore errors here.
        let _ = run_git(project_root, &["branch", "-D", branch]);
    }
    Ok(())
}
