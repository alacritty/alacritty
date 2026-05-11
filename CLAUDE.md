# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository layout

This is a Cargo workspace. There are five crates, but only one is original work:

- `alacritree/` — **the only crate this fork actually changes.** A small egui/eframe app that hosts `alacritty_terminal` and adds a worktree-aware sidebar. All Claude-edited code should live here unless the user explicitly says otherwise.
- `alacritty/`, `alacritty_terminal/`, `alacritty_config/`, `alacritty_config_derive/` — vendored upstream alacritty. Treat as read-only dependencies. The `alacritty` GUI binary (winit/OpenGL) is **not** what this fork ships; we only use `alacritty_terminal` (the headless PTY + VT parser + grid).

`CONTRIBUTING.md` is the upstream alacritty contributing guide, kept for the vendored crates' historical context. It does not constrain work on `alacritree/`.

## Build / run

```sh
cargo run -p alacritree            # debug build of the GUI
cargo build -p alacritree --release
cargo check -p alacritree          # fast type-check loop
cargo fmt                          # rustfmt is enforced (see rustfmt.toml)
cargo test -p alacritree           # the alacritree crate currently has no tests
```

The workspace MSRV is 1.85 (edition 2024). The root `Makefile` is upstream alacritty's macOS bundling script; it is **not** wired up to alacritree.

There is a `[patch.crates-io]` pin on `x11-clipboard` in the root `Cargo.toml` (TODO from upstream) — leave it alone unless asked.

## Big-picture architecture

`alacritree` is an egui app that owns N PTY-backed terminal sessions and routes input/paint through a custom grid renderer. The pieces:

- `main.rs` — `eframe::run_native`, env_logger setup. Window opacity comes from config; transparency is a `ViewportBuilder` flag, so toggling it requires restart.
- `app.rs` — `AlacritreeApp` is the `eframe::App`. Owns `Vec<Session>`, the project list, the per-workspace active-session map, and the cached `Theme`. **Workspace model:** a `WorkspaceKey = Option<PathBuf>` — `None` is the "home" tab (sessions inherit `$PWD`), `Some(path)` is a worktree. The active session for a workspace persists across switches; sessions are *not* killed when you switch away. Sidebars: left = projects/worktrees, right = git status. Both are toggleable and persisted.
- `session.rs` — wraps `alacritty_terminal::event_loop::EventLoop`. Each `Session` has its own PTY, its own background read/write thread, and its own monotonic `window_id` (alacritty routes OSC 7 / signal events by id, so ids must be unique). `EventProxy` bridges terminal events into an `mpsc` + `egui::Context::request_repaint`. `Drop` sends `Msg::Shutdown` — don't bypass this.
- `terminal_view.rs` — the custom grid painter. Computes cell size from the egui font, resizes the session to fit, drains pending PTY events (`Title`, `ChildExit`, `PtyWrite`), and paints the grid cell-by-cell. Input goes through `input::event_to_bytes`.
- `input.rs` — translates `egui::Event` → terminal byte sequences (CSI/SS3 for arrows/F-keys, `ESC + key` for Alt, control bytes for Ctrl-letter). `Event::Text` is preferred for printable input because it handles dead keys / IME.
- `bindings.rs` — parses alacritty's `[[keyboard.bindings]]` TOML into egui `KeyboardShortcut`s. Vi/search-mode bindings are dropped (no mode tracking). `BindingAction::Chars` writes raw bytes; `Named` triggers app-level actions (paste, scroll, font-size, quit, …).
- `config.rs` — loads `alacritty.toml` then deep-merges `alacritree.toml` over it using **alacritty's merge semantics**: arrays *concatenate* (so `[[keyboard.bindings]]` in alacritree.toml *adds to* upstream bindings), tables merge recursively, primitives replace. Search path mirrors alacritty: `$XDG_CONFIG_HOME/alacritty/`, `~/.config/alacritty/`, `~/.alacritty.toml`, `/etc/alacritty/`. alacritree-only options (sidebar colors, etc.) live under `[ui]`.
- `colors.rs` — converts alacritty's `Rgb` + `AnsiColor` (Named/Spec/Indexed) to `egui::Color32`, applying the 256-color palette and bright/dim variants.
- `fonts.rs` — loads a system monospace font via `fontdb` and registers it with egui.
- `projects.rs` — `Project::discover(path)` opens with `git2`, lists worktrees via `repo.worktrees()`, and detects the default branch (config `init.defaultBranch` → `refs/remotes/origin/HEAD` → fallback to `main`/`master`). Non-git roots get a single pseudo-worktree pointing at themselves so the user can still spawn a shell there.
- `git_status.rs` — `StatusCache` per worktree, throttled to 1.5 s. Computes staged/unstaged file lists and a diff-stat against the project's default branch for the right sidebar.
- `state.rs` — minimal persistence to `$XDG_CONFIG_HOME/alacritree/state.toml`: project roots, expanded state, sidebar visibility. Serialized with `toml`. Failures are logged and ignored — never panic on missing/corrupt state.

## Conventions specific to this fork

- Two TOML files: `alacritty.toml` (shared with the alacritty terminal — palette, cursor, scrolling, shell, key bindings) and `alacritree.toml` (sidebar/UI overrides under `[ui]`). When adding a config field, decide whether it belongs in the shared file or the alacritree-only file, and document it in the relevant `Raw*` struct in `config.rs`.
- Sessions outlive workspace switches. Don't introduce code that drops a `Session` just because it isn't visible.
- `EventProxy::send_event` calls `request_repaint` — this is what wakes the egui loop on PTY output. Anything that produces terminal events on a background thread must go through an `EventProxy` (or otherwise call `request_repaint`) or it will appear to hang until the next input event.
- Logs use the `log` crate. `egui_winit::clipboard=error` is filtered down by default in `main.rs` because cold X11 clipboard probes warn noisily; keep that filter unless you have a reason to remove it.
- Comments in `alacritree/` follow the "explain the *why*, not the *what*" pattern already in the file headers (e.g. `state.rs`, `config.rs`, `projects.rs`). Match that style — short, reason-giving, no rote restatements of the code.
- Always follow clean code practices: clear naming, small focused functions, no dead code, no premature abstractions. Never add useless comments (rote what-restatements, "added by X", task references), and never remove existing comments unless they are demonstrably wrong or made obsolete by the change you are making.
- Always use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages (`feat:`, `fix:`, `refactor:`, `docs:`, `chore:`, etc., with an optional scope like `feat(sidebar):`). Keep the subject line imperative and under ~72 chars.
