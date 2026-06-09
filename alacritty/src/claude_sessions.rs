//! Discovery of Claude Code session transcripts for a project folder.
//!
//! Claude Code stores each conversation as a line-delimited JSON transcript at
//! `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl`, where `<encoded-cwd>` is the project's
//! absolute path with `/` and `.` replaced by `-`. This module enumerates those transcripts for a
//! given project root and derives a short label (the first user prompt) for each, so the in-app
//! sidebar can list them and resume one in a new tab.
//!
//! Transcripts can be large (megabytes), but the label lives near the top, so only the first few
//! lines of each file are read.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Maximum number of sessions surfaced per project.
const MAX_SESSIONS_SHOWN: usize = 12;
/// Lines read from the head of a transcript while searching for the first user prompt.
const HEAD_LINES: usize = 40;

/// A Claude Code session transcript that can be resumed.
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    /// Session UUID (the transcript's file stem). Validated to a UUID-ish charset.
    pub id: String,
    /// Human label: the first user prompt, collapsed to one line, or `"(no prompt)"`.
    pub label: String,
}

/// Enumerate the Claude Code sessions for `root`, newest first, capped at [`MAX_SESSIONS_SHOWN`].
///
/// Returns an empty list when there is no home directory, no `~/.claude/projects/<root>` directory,
/// or the directory can't be read.
pub fn sessions_for(root: &Path) -> Vec<ClaudeSession> {
    let Some(dir) = project_dir(root) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };

    // Collect valid transcripts with their mtime, then sort newest-first and cap before parsing
    // labels — so we never read the head of more files than we display.
    let mut found: Vec<(String, PathBuf, SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if !is_session_id(stem) {
            continue;
        }
        let mtime =
            entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        found.push((stem.to_owned(), path, mtime));
    }
    found.sort_by(|a, b| b.2.cmp(&a.2));
    found.truncate(MAX_SESSIONS_SHOWN);

    found
        .into_iter()
        .map(|(id, path, _)| {
            let label = read_label(&path).unwrap_or_else(|| "(no prompt)".to_owned());
            ClaudeSession { id, label }
        })
        .collect()
}

/// `~/.claude/projects/<encoded>`, where `<encoded>` mirrors Claude Code's cwd encoding (`/` and
/// `.` replaced by `-`).
fn project_dir(root: &Path) -> Option<PathBuf> {
    let encoded: String = root
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    Some(home::home_dir()?.join(".claude").join("projects").join(encoded))
}

/// Whether `s` looks like a session UUID (hex digits and hyphens, plausible length). Used to reject
/// stray filenames before they ever reach a `claude --resume <id>` command.
fn is_session_id(s: &str) -> bool {
    (10..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Read the first user prompt from the head of a transcript as a one-line label.
fn read_label(path: &Path) -> Option<String> {
    let reader = BufReader::new(File::open(path).ok()?);
    for line in reader.lines().take(HEAD_LINES) {
        let Ok(line) = line else { continue };
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        if value.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        let text = value.get("message").and_then(|m| m.get("content")).and_then(content_text)?;
        let label = collapse_whitespace(&text);
        if !label.is_empty() {
            return Some(label);
        }
        return None;
    }
    None
}

/// Extract text from a message `content`, which is either a plain string or an array of content
/// blocks (`[{ "type": "text", "text": "…" }, …]`).
fn content_text(content: &serde_json::Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    let blocks = content.as_array()?;
    for block in blocks {
        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
            return Some(text.to_owned());
        }
    }
    None
}

/// Collapse all runs of whitespace (including newlines) to single spaces and trim, so a multi-line
/// prompt becomes a single tidy label.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_validation() {
        assert!(is_session_id("f152b8af-a2b7-4fa8-8992-33ebcbc22e16"));
        assert!(!is_session_id("not a uuid"));
        assert!(!is_session_id("../evil"));
        assert!(!is_session_id("rm -rf /"));
        assert!(!is_session_id("short"));
    }

    #[test]
    fn content_as_string() {
        let v: serde_json::Value = serde_json::json!(" 运行下看看现在的效果 ");
        assert_eq!(content_text(&v).as_deref(), Some(" 运行下看看现在的效果 "));
    }

    #[test]
    fn content_as_blocks() {
        let v: serde_json::Value =
            serde_json::json!([{ "type": "text", "text": "hello there" }]);
        assert_eq!(content_text(&v).as_deref(), Some("hello there"));
    }

    #[test]
    fn whitespace_is_collapsed() {
        assert_eq!(collapse_whitespace("  a\n  b\t c  "), "a b c");
    }
}
