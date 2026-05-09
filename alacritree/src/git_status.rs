//! Working-tree status + a summary of changes vs the project's default branch.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use git2::{Delta, DiffOptions, Repository, Status, StatusOptions};

const REFRESH_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

impl ChangeKind {
    pub fn glyph(&self) -> &'static str {
        match self {
            ChangeKind::Added => "A",
            ChangeKind::Modified => "M",
            ChangeKind::Deleted => "D",
            ChangeKind::Renamed => "R",
            ChangeKind::Untracked => "?",
            ChangeKind::Conflicted => "!",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffStat {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DirtyCounts {
    pub staged: usize,
    pub modified: usize,
    pub untracked: usize,
}

impl DirtyCounts {
    pub fn is_dirty(&self) -> bool {
        self.staged + self.modified + self.untracked > 0
    }
}

/// Cheap dirty check used by the delete modal: avoids the branch-diff work
/// that `compute` does, since we only need to know whether `git worktree
/// remove` will refuse the path.
pub fn dirty_counts(path: &Path) -> DirtyCounts {
    let Ok(repo) = Repository::open(path) else {
        return DirtyCounts::default();
    };
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    let Ok(statuses) = repo.statuses(Some(&mut opts)) else {
        return DirtyCounts::default();
    };
    let mut counts = DirtyCounts::default();
    let staged_mask = Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE;
    let modified_mask =
        Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE;
    for entry in statuses.iter() {
        let s = entry.status();
        if s.intersects(staged_mask) {
            counts.staged += 1;
        }
        if s.contains(Status::WT_NEW) {
            counts.untracked += 1;
        } else if s.intersects(modified_mask) {
            counts.modified += 1;
        }
    }
    counts
}

#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub default_branch: Option<String>,
    pub default_branch_resolved: Option<String>,
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub branch_diff: Vec<DiffStat>,
    pub error: Option<String>,
}

pub struct StatusCache {
    path: PathBuf,
    last: Option<(Instant, GitStatus)>,
}

impl StatusCache {
    pub fn new(path: PathBuf) -> Self {
        Self { path, last: None }
    }

    pub fn get(&mut self) -> &GitStatus {
        let needs_refresh =
            self.last.as_ref().map_or(true, |(when, _)| when.elapsed() > REFRESH_INTERVAL);
        if needs_refresh {
            let status = compute(&self.path, None);
            self.last = Some((Instant::now(), status));
        }
        &self.last.as_ref().unwrap().1
    }

    pub fn force_refresh(&mut self, default_branch_hint: Option<&str>) {
        let status = compute(&self.path, default_branch_hint);
        self.last = Some((Instant::now(), status));
    }
}

pub fn compute(path: &Path, default_branch_hint: Option<&str>) -> GitStatus {
    match compute_inner(path, default_branch_hint) {
        Ok(s) => s,
        Err(e) => GitStatus { error: Some(e.to_string()), ..Default::default() },
    }
}

fn compute_inner(path: &Path, default_branch_hint: Option<&str>) -> Result<GitStatus, git2::Error> {
    let repo = Repository::open(path)?;

    let branch = current_branch_name(&repo);
    let default_branch =
        default_branch_hint.map(|s| s.to_string()).or_else(|| detect_default_branch(&repo));

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();

    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.renames_head_to_index(true);
    opts.renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    for entry in statuses.iter() {
        let path_str = entry.path().unwrap_or("").to_string();
        let status = entry.status();
        if let Some(kind) = staged_kind(status) {
            staged.push(FileChange { path: path_str.clone(), kind });
        }
        if let Some(kind) = unstaged_kind(status) {
            unstaged.push(FileChange { path: path_str, kind });
        }
    }

    let (branch_diff, default_branch_resolved) = match default_branch.as_deref() {
        Some(name) => match diff_against_branch(&repo, name) {
            Ok((stats, resolved)) => (stats, Some(resolved)),
            Err(_) => (Vec::new(), None),
        },
        None => (Vec::new(), None),
    };

    Ok(GitStatus {
        branch,
        default_branch,
        default_branch_resolved,
        staged,
        unstaged,
        branch_diff,
        error: None,
    })
}

fn current_branch_name(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if head.is_branch() {
        head.shorthand().map(|s| s.to_string())
    } else {
        head.target().map(|oid| oid.to_string().chars().take(7).collect())
    }
}

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
            if let Some(name) = target.strip_prefix("refs/remotes/origin/") {
                return Some(name.to_string());
            }
        }
    }
    for c in ["main", "master"] {
        if repo.find_reference(&format!("refs/heads/{c}")).is_ok() {
            return Some(c.to_string());
        }
    }
    None
}

fn staged_kind(s: Status) -> Option<ChangeKind> {
    if s.is_conflicted() {
        return Some(ChangeKind::Conflicted);
    }
    if s.contains(Status::INDEX_NEW) {
        return Some(ChangeKind::Added);
    }
    if s.contains(Status::INDEX_DELETED) {
        return Some(ChangeKind::Deleted);
    }
    if s.contains(Status::INDEX_RENAMED) {
        return Some(ChangeKind::Renamed);
    }
    if s.intersects(Status::INDEX_MODIFIED | Status::INDEX_TYPECHANGE) {
        return Some(ChangeKind::Modified);
    }
    None
}

fn unstaged_kind(s: Status) -> Option<ChangeKind> {
    if s.contains(Status::WT_NEW) {
        return Some(ChangeKind::Untracked);
    }
    if s.contains(Status::WT_DELETED) {
        return Some(ChangeKind::Deleted);
    }
    if s.contains(Status::WT_RENAMED) {
        return Some(ChangeKind::Renamed);
    }
    if s.intersects(Status::WT_MODIFIED | Status::WT_TYPECHANGE) {
        return Some(ChangeKind::Modified);
    }
    None
}

/// Diff against the merge base, not the branch tip, so local-only commits
/// still appear when the default branch hasn't moved.
fn diff_against_branch(
    repo: &Repository,
    branch: &str,
) -> Result<(Vec<DiffStat>, String), git2::Error> {
    let (base_commit, resolved) = resolve_base_commit(repo, branch)?;
    let head_commit = repo.head()?.peel_to_commit()?;

    let merge_base_oid = repo.merge_base(base_commit.id(), head_commit.id())?;
    let merge_base_commit = repo.find_commit(merge_base_oid)?;

    let base_tree = merge_base_commit.tree()?;
    let head_tree = head_commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(false).recurse_untracked_dirs(false);
    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut opts))?;

    let mut stats = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if matches!(delta.status(), Delta::Unmodified | Delta::Ignored) {
                return true;
            }
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            stats.push(DiffStat { path, additions: 0, deletions: 0 });
            true
        },
        None,
        None,
        None,
    )?;

    for i in 0..diff.deltas().len() {
        if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, i) {
            let (_, additions, deletions) = patch.line_stats().unwrap_or((0, 0, 0));
            if let Some(stat) = stats.get_mut(i) {
                stat.additions = additions;
                stat.deletions = deletions;
            }
        }
    }

    Ok((stats, resolved))
}

fn resolve_base_commit<'a>(
    repo: &'a Repository,
    branch: &str,
) -> Result<(git2::Commit<'a>, String), git2::Error> {
    let candidates = [format!("refs/remotes/origin/{branch}"), format!("refs/heads/{branch}")];
    for refname in &candidates {
        if let Ok(reference) = repo.find_reference(refname) {
            if let Ok(commit) = reference.peel_to_commit() {
                return Ok((commit, refname.clone()));
            }
        }
    }
    Err(git2::Error::from_str(&format!("default branch '{branch}' not found")))
}
