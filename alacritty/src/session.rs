//! Persisted session state.
//!
//! Records the projects, tabs and windows that are open so they can be restored the next time
//! Alacritty launches. The state lives in a small SQLite database under the platform state
//! directory (e.g. `~/.local/state/alacritty/session.db` on Unix).
//!
//! Only the *layout* is persisted — for each tab we store its working directory and title, for
//! each project its name and root folder, plus the window's pixel size and active project/tab.
//! Live shell processes, scrollback and in-memory environment cannot be restored; restored tabs
//! simply spawn a fresh shell in the saved directory.

use std::path::PathBuf;

use log::{error, warn};
use rusqlite::{Connection, params};

/// Schema version. Bumped whenever the table layout changes; a mismatch drops and recreates the
/// tables (session restore is best-effort, so losing one prior session on upgrade is acceptable).
const SCHEMA_VERSION: i64 = 2;

/// A single restored tab.
#[derive(Debug, Clone, Default)]
pub struct SavedTab {
    /// Working directory the tab's shell should be spawned in.
    pub working_directory: Option<PathBuf>,
    /// Last title reported by the tab, used as the initial tab-bar label.
    pub title: String,
}

/// A single restored project with its tabs.
#[derive(Debug, Clone)]
pub struct SavedProject {
    /// Display name shown in the sidebar.
    pub name: String,
    /// Folder the project is rooted at (new tabs default here). `None` for the default project.
    pub root: Option<PathBuf>,
    /// Index of the tab that was focused within this project.
    pub active_tab: usize,
    /// Tabs in display order.
    pub tabs: Vec<SavedTab>,
}

/// A single restored window with its projects.
#[derive(Debug, Clone)]
pub struct SavedWindow {
    /// Stable per-window key, reused across restores so updates target the right row.
    pub key: i64,
    /// Window inner width in physical pixels.
    pub width: u32,
    /// Window inner height in physical pixels.
    pub height: u32,
    /// Index of the project that was active.
    pub active_project: usize,
    /// Projects in display order.
    pub projects: Vec<SavedProject>,
}

/// SQLite-backed store for the last session's window/project/tab layout.
pub struct SessionStore {
    conn: Connection,
}

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS windows (
        key    INTEGER PRIMARY KEY,
        width  INTEGER NOT NULL,
        height INTEGER NOT NULL,
        active INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS projects (
        window_key INTEGER NOT NULL,
        position   INTEGER NOT NULL,
        name       TEXT NOT NULL,
        root       TEXT,
        active     INTEGER NOT NULL,
        PRIMARY KEY (window_key, position)
    );
    CREATE TABLE IF NOT EXISTS tabs (
        window_key       INTEGER NOT NULL,
        project_position INTEGER NOT NULL,
        position         INTEGER NOT NULL,
        cwd              TEXT,
        title            TEXT NOT NULL,
        PRIMARY KEY (window_key, project_position, position)
    );
";

impl SessionStore {
    /// Open (creating if necessary) the session database.
    ///
    /// Returns `None` if the state directory or database is unavailable; session persistence is a
    /// best-effort feature and must never block startup.
    pub fn open() -> Option<Self> {
        let path = Self::db_path()?;
        let conn = match Connection::open(&path) {
            Ok(conn) => conn,
            Err(err) => {
                warn!("session: could not open {path:?}: {err}");
                return None;
            },
        };

        if let Err(err) = Self::migrate(&conn) {
            warn!("session: could not initialise schema: {err}");
            return None;
        }

        Some(Self { conn })
    }

    /// Bring the database schema up to [`SCHEMA_VERSION`].
    ///
    /// On a version mismatch the tables are dropped and recreated. This loses at most one prior
    /// session's restore data, which is acceptable for a best-effort feature.
    fn migrate(conn: &Connection) -> rusqlite::Result<()> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS tabs;
                 DROP TABLE IF EXISTS projects;
                 DROP TABLE IF EXISTS windows;",
            )?;
        }
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    /// Resolve the database path, creating its parent directory.
    ///
    /// Stored under the XDG state directory on Unix (`~/.local/state/alacritty`) and the data
    /// directory on Windows (`%APPDATA%\alacritty`).
    fn db_path() -> Option<PathBuf> {
        #[cfg(not(windows))]
        {
            match xdg::BaseDirectories::with_prefix("alacritty").place_state_file("session.db") {
                Ok(path) => Some(path),
                Err(err) => {
                    warn!("session: could not resolve state directory: {err}");
                    None
                },
            }
        }

        #[cfg(windows)]
        {
            let dir = dirs::data_dir()?.join("alacritty");
            if let Err(err) = std::fs::create_dir_all(&dir) {
                warn!("session: could not create data directory {dir:?}: {err}");
                return None;
            }
            Some(dir.join("session.db"))
        }
    }

    /// Highest window key currently stored, or 0 if empty.
    ///
    /// Used to seed the key counter so freshly opened windows never collide with restored ones.
    pub fn max_key(&self) -> i64 {
        self.conn
            .query_row("SELECT COALESCE(MAX(key), 0) FROM windows", [], |row| row.get(0))
            .unwrap_or(0)
    }

    /// Insert or update a window and replace its project/tab rows.
    pub fn save_window(&self, win: &SavedWindow) {
        if let Err(err) = self.save_window_inner(win) {
            error!("session: failed to save window {}: {err}", win.key);
        }
    }

    fn save_window_inner(&self, win: &SavedWindow) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO windows (key, width, height, active) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET width = ?2, height = ?3, active = ?4",
            params![win.key, win.width, win.height, win.active_project as i64],
        )?;
        tx.execute("DELETE FROM tabs WHERE window_key = ?1", params![win.key])?;
        tx.execute("DELETE FROM projects WHERE window_key = ?1", params![win.key])?;
        {
            let mut proj_stmt = tx.prepare(
                "INSERT INTO projects (window_key, position, name, root, active)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut tab_stmt = tx.prepare(
                "INSERT INTO tabs (window_key, project_position, position, cwd, title)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (pp, project) in win.projects.iter().enumerate() {
                let root = project.root.as_ref().map(|p| p.to_string_lossy().into_owned());
                proj_stmt.execute(params![
                    win.key,
                    pp as i64,
                    project.name,
                    root,
                    project.active_tab as i64
                ])?;
                for (tp, tab) in project.tabs.iter().enumerate() {
                    let cwd =
                        tab.working_directory.as_ref().map(|p| p.to_string_lossy().into_owned());
                    tab_stmt.execute(params![win.key, pp as i64, tp as i64, cwd, tab.title])?;
                }
            }
        }
        tx.commit()
    }

    /// Forget a window (e.g. when the user closes it while other windows remain).
    pub fn remove_window(&self, key: i64) {
        let result = (|| -> rusqlite::Result<()> {
            self.conn.execute("DELETE FROM tabs WHERE window_key = ?1", params![key])?;
            self.conn.execute("DELETE FROM projects WHERE window_key = ?1", params![key])?;
            self.conn.execute("DELETE FROM windows WHERE key = ?1", params![key])?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("session: failed to remove window {key}: {err}");
        }
    }

    /// Load every saved window in key order, each with its projects and their tabs in order.
    pub fn load(&self) -> Vec<SavedWindow> {
        match self.load_inner() {
            Ok(windows) => windows,
            Err(err) => {
                error!("session: failed to load: {err}");
                Vec::new()
            },
        }
    }

    fn load_inner(&self) -> rusqlite::Result<Vec<SavedWindow>> {
        let mut win_stmt =
            self.conn.prepare("SELECT key, width, height, active FROM windows ORDER BY key")?;
        let mut windows: Vec<SavedWindow> = win_stmt
            .query_map([], |row| {
                Ok(SavedWindow {
                    key: row.get(0)?,
                    width: row.get::<_, i64>(1)? as u32,
                    height: row.get::<_, i64>(2)? as u32,
                    active_project: row.get::<_, i64>(3)?.max(0) as usize,
                    projects: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut proj_stmt = self.conn.prepare(
            "SELECT position, name, root, active FROM projects WHERE window_key = ?1 ORDER BY position",
        )?;
        let mut tab_stmt = self.conn.prepare(
            "SELECT cwd, title FROM tabs WHERE window_key = ?1 AND project_position = ?2 ORDER BY position",
        )?;

        for win in &mut windows {
            // Collect this window's projects (keyed by their stored position).
            let mut projects: Vec<(i64, SavedProject)> = proj_stmt
                .query_map(params![win.key], |row| {
                    let root: Option<String> = row.get(2)?;
                    let project = SavedProject {
                        name: row.get(1)?,
                        root: root.map(PathBuf::from),
                        active_tab: row.get::<_, i64>(3)?.max(0) as usize,
                        tabs: Vec::new(),
                    };
                    Ok((row.get::<_, i64>(0)?, project))
                })?
                .collect::<rusqlite::Result<_>>()?;

            for (position, project) in &mut projects {
                project.tabs = tab_stmt
                    .query_map(params![win.key, *position], |row| {
                        let cwd: Option<String> = row.get(0)?;
                        Ok(SavedTab {
                            working_directory: cwd.map(PathBuf::from),
                            title: row.get(1)?,
                        })
                    })?
                    .collect::<rusqlite::Result<_>>()?;
            }

            // Drop projects with no tabs; they cannot be restored.
            win.projects =
                projects.into_iter().map(|(_, p)| p).filter(|p| !p.tabs.is_empty()).collect();
        }

        // Drop any window that ended up with no projects.
        windows.retain(|win| !win.projects.is_empty());
        Ok(windows)
    }

    /// In-memory store for tests, exercising the same migration path as [`Self::open`].
    #[cfg(test)]
    fn in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        Self::migrate(&conn).unwrap();
        Self { conn }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(cwd: &str, title: &str) -> SavedTab {
        SavedTab { working_directory: Some(PathBuf::from(cwd)), title: title.into() }
    }

    fn project(name: &str, root: &str, active_tab: usize, tabs: Vec<SavedTab>) -> SavedProject {
        SavedProject { name: name.into(), root: Some(PathBuf::from(root)), active_tab, tabs }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let store = SessionStore::in_memory();
        let win = SavedWindow {
            key: 1,
            width: 1280,
            height: 720,
            active_project: 1,
            projects: vec![
                project("app", "/app", 0, vec![tab("/app", "a0")]),
                project("docs", "/docs", 1, vec![tab("/docs", "d0"), tab("/docs/x", "d1")]),
            ],
        };
        store.save_window(&win);

        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].key, 1);
        assert_eq!(loaded[0].width, 1280);
        assert_eq!(loaded[0].active_project, 1);
        assert_eq!(loaded[0].projects.len(), 2);
        assert_eq!(loaded[0].projects[1].name, "docs");
        assert_eq!(loaded[0].projects[1].root, Some(PathBuf::from("/docs")));
        assert_eq!(loaded[0].projects[1].active_tab, 1);
        assert_eq!(loaded[0].projects[1].tabs.len(), 2);
        assert_eq!(loaded[0].projects[1].tabs[1].title, "d1");
    }

    #[test]
    fn save_window_replaces_projects_and_tabs() {
        let store = SessionStore::in_memory();
        store.save_window(&SavedWindow {
            key: 1,
            width: 1,
            height: 1,
            active_project: 0,
            projects: vec![
                project("a", "/a", 0, vec![tab("/a", "a0"), tab("/a", "a1")]),
                project("b", "/b", 0, vec![tab("/b", "b0")]),
            ],
        });
        // Re-save with fewer projects/tabs; stale rows must not linger.
        store.save_window(&SavedWindow {
            key: 1,
            width: 1,
            height: 1,
            active_project: 0,
            projects: vec![project("x", "/x", 0, vec![tab("/x", "x0")])],
        });

        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].projects.len(), 1);
        assert_eq!(loaded[0].projects[0].name, "x");
        assert_eq!(loaded[0].projects[0].tabs.len(), 1);
    }

    #[test]
    fn remove_window_and_max_key() {
        let store = SessionStore::in_memory();
        assert_eq!(store.max_key(), 0);

        store.save_window(&SavedWindow {
            key: 3,
            width: 1,
            height: 1,
            active_project: 0,
            projects: vec![project("a", "/a", 0, vec![tab("/a", "a")])],
        });
        store.save_window(&SavedWindow {
            key: 7,
            width: 1,
            height: 1,
            active_project: 0,
            projects: vec![project("b", "/b", 0, vec![tab("/b", "b")])],
        });
        assert_eq!(store.max_key(), 7);

        store.remove_window(3);
        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].key, 7);
        // Removed window's project/tab rows are gone too.
        let project_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM projects WHERE window_key = 3", [], |r| r.get(0))
            .unwrap();
        assert_eq!(project_count, 0);
    }

    #[test]
    fn projectless_windows_are_dropped_on_load() {
        let store = SessionStore::in_memory();
        // A window row with no project rows (corrupt/partial) must not be returned.
        store
            .conn
            .execute("INSERT INTO windows (key, width, height, active) VALUES (5, 1, 1, 0)", [])
            .unwrap();
        assert!(store.load().is_empty());
    }

    #[test]
    fn legacy_schema_is_reset() {
        // Simulate a pre-v2 database (flat `tabs`, no `projects`, old user_version).
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE windows (key INTEGER PRIMARY KEY, width INTEGER, height INTEGER, active INTEGER);
             CREATE TABLE tabs (window_key INTEGER, position INTEGER, cwd TEXT, title TEXT);
             INSERT INTO windows VALUES (1, 800, 600, 0);
             INSERT INTO tabs VALUES (1, 0, '/old', 'legacy');",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1i64).unwrap();

        // Migrating to the current version drops the legacy data and installs the new schema.
        SessionStore::migrate(&conn).unwrap();
        let store = SessionStore { conn };
        assert!(store.load().is_empty());

        // The new `projects` table now exists and is usable.
        store.save_window(&SavedWindow {
            key: 1,
            width: 1,
            height: 1,
            active_project: 0,
            projects: vec![project("p", "/p", 0, vec![tab("/p", "t")])],
        });
        assert_eq!(store.load().len(), 1);
    }
}
