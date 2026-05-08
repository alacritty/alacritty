//! Discovery of projects and their git worktrees.
//!
//! A "project" here is a directory the user has added to the sidebar — usually
//! a git repository.  For each project we list:
//!  - the main checkout (the working directory of the repo itself), and
//!  - any linked worktrees (`git worktree list`).
//!
//! We re-read this on demand rather than caching across long periods because
//! worktrees come and go between sessions.

use std::path::PathBuf;

use git2::Repository;

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub default_branch: Option<String>,
    pub worktrees: Vec<Worktree>,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_main: bool,
}

impl Project {
    /// Open `root` as a git repo and enumerate its main + linked worktrees.
    /// If `root` isn't a git repository, returns a project with a single
    /// pseudo-worktree for the directory itself — the user can still use the
    /// sidebar to spawn a shell rooted there.
    pub fn discover(root: PathBuf) -> Self {
        let name = root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());

        match Repository::open(&root) {
            Ok(repo) => Self::from_repo(root, name, &repo),
            Err(_) => Project {
                worktrees: vec![Worktree {
                    name: name.clone(),
                    path: root.clone(),
                    branch: None,
                    is_main: true,
                }],
                root,
                name,
                default_branch: None,
                expanded: true,
            },
        }
    }

    fn from_repo(root: PathBuf, name: String, repo: &Repository) -> Self {
        let main_path = repo
            .workdir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| root.clone());

        let mut worktrees = Vec::new();
        worktrees.push(Worktree {
            name: "main".to_string(),
            path: main_path.clone(),
            branch: current_branch(repo),
            is_main: true,
        });

        if let Ok(names) = repo.worktrees() {
            for name in names.iter().flatten() {
                if let Ok(wt) = repo.find_worktree(name) {
                    let path = wt.path().to_path_buf();
                    let branch = Repository::open(&path)
                        .ok()
                        .and_then(|wt_repo| current_branch(&wt_repo));
                    worktrees.push(Worktree {
                        name: name.to_string(),
                        path,
                        branch,
                        is_main: false,
                    });
                }
            }
        }

        Project {
            default_branch: detect_default_branch(repo),
            worktrees,
            root,
            name,
            expanded: true,
        }
    }

    pub fn refresh(&mut self) {
        let updated = Project::discover(self.root.clone());
        self.worktrees = updated.worktrees;
        self.default_branch = updated.default_branch;
    }
}

fn current_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if head.is_branch() {
        head.shorthand().map(|s| s.to_string())
    } else {
        // Detached HEAD: show the short OID.
        head.target().map(|oid| {
            let s = oid.to_string();
            s.chars().take(7).collect()
        })
    }
}

/// Best-effort detection of the repository's default branch.
///
/// Order: `init.defaultBranch` config → `refs/remotes/origin/HEAD` → presence of
/// `main` / `master`.  Returns the branch name (without `refs/heads/`) or
/// `None` if nothing fits.
fn detect_default_branch(repo: &Repository) -> Option<String> {
    if let Ok(cfg) = repo.config() {
        if let Ok(name) = cfg.get_string("init.defaultBranch") {
            if !name.is_empty() {
                return Some(name);
            }
        }
    }

    if let Ok(reference) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Some(target) = reference.symbolic_target() {
            // Strip the leading "refs/remotes/origin/".
            if let Some(name) = target.strip_prefix("refs/remotes/origin/") {
                return Some(name.to_string());
            }
        }
    }

    for candidate in ["main", "master"] {
        if repo
            .find_reference(&format!("refs/heads/{candidate}"))
            .is_ok()
        {
            return Some(candidate.to_string());
        }
    }
    None
}

