# Keyboard shortcuts

Alacritree has two layers of shortcuts:

1. **Built-in app shortcuts** — hard-coded, not configurable. They drive
   alacritree-specific UI (sidebars, workspaces, session list, quit dialog).
2. **Configurable terminal bindings** — parsed from your
   `[[keyboard.bindings]]` tables in `alacritty.toml` / `alacritree.toml`, with
   a set of defaults that mirror alacritty.

When both layers would match the same key, the built-in shortcut wins.

---

## Built-in app shortcuts

These cannot be rebound today.

| Shortcut             | Action                                                |
| -------------------- | ----------------------------------------------------- |
| `Ctrl+B`             | Toggle the left (projects/worktrees) sidebar          |
| `Ctrl+G`             | Toggle the right (git status) sidebar                 |
| `Ctrl+T`             | Open a new shell session in the current workspace     |
| `Ctrl+Tab`           | Cycle to the next session in the current workspace    |
| `Ctrl+Shift+Tab`     | Cycle to the previous session                         |
| `Ctrl+Alt+Right`     | Switch to the next workspace (home / worktrees)       |
| `Ctrl+Alt+Left`      | Switch to the previous workspace                      |
| `Ctrl+Q`             | Open the quit confirmation dialog                     |
| `Enter`              | Confirm a modal (quit, delete worktree, create branch)|
| `Escape`             | Cancel a modal                                        |

Modal-specific keys (`Enter`/`Escape`) only fire while a modal is open and never
reach the terminal grid.

---

## Configurable terminal bindings

These are parsed from `[[keyboard.bindings]]` and matched against egui key
events before the terminal sees them. Alacritty's own default set is preloaded,
and your TOML entries are checked first — so your config overrides any default.

### Defaults on every platform

| Shortcut             | Action                                                |
| -------------------- | ----------------------------------------------------- |
| `Ctrl+Shift+V`       | Paste from the clipboard                              |
| `Ctrl+Shift+C`       | Copy the current selection                            |
| `Shift+Insert`       | Paste from the primary (X11) selection                |
| `Ctrl+0`             | Reset font size                                       |
| `Ctrl+=` / `Ctrl++`  | Increase font size                                    |
| `Ctrl+-`             | Decrease font size                                    |
| `Shift+Home`         | Scroll to the top of the scrollback                   |
| `Shift+End`          | Scroll to the bottom                                  |
| `Shift+PageUp`       | Scroll one page up                                    |
| `Shift+PageDown`     | Scroll one page down                                  |
| `Shift+Tab`          | Send `CSI Z` (reverse tab — readline/vim)             |
| `Alt+Shift+Tab`      | Send `ESC` + `CSI Z`                                  |

### Additional defaults on macOS

| Shortcut             | Action                                                |
| -------------------- | ----------------------------------------------------- |
| `Cmd+V`              | Paste                                                 |
| `Cmd+C`              | Copy                                                  |
| `Cmd+N` / `Cmd+T`    | Open a new shell session in the current workspace     |
| `Cmd+0`              | Reset font size                                       |
| `Cmd+=` / `Cmd++`    | Increase font size                                    |
| `Cmd+-`              | Decrease font size                                    |
| `Cmd+Shift+]`        | Next session in the current workspace                 |
| `Cmd+Shift+[`        | Previous session                                      |
| `Cmd+1` … `Cmd+8`    | Select the Nth session in the current workspace       |
| `Cmd+9`              | Select the last session                               |
| `Ctrl+Cmd+F`         | Toggle fullscreen                                     |
| `Cmd+M`              | Minimize the window                                   |
| `Cmd+K`              | Clear the scrollback buffer                           |
| `Cmd+Q`              | Open the quit confirmation dialog                     |

---

## Supported actions

Use any of these as the `action = "..."` value in a `[[keyboard.bindings]]`
entry. Names match alacritty's own action names, so existing configs port over.

### Clipboard

- `Paste` — paste from the system clipboard.
- `Copy` — copy the current selection to the clipboard. *(Selection isn't wired
  up in the alacritree grid yet; this becomes a no-op when there's nothing
  selected.)*
- `PasteSelection` — paste from the primary (X11) selection.

### Font size

- `IncreaseFontSize`
- `DecreaseFontSize`
- `ResetFontSize`

### Scrolling

- `ScrollPageUp` / `ScrollPageDown`
- `ScrollHalfPageUp` / `ScrollHalfPageDown`
- `ScrollLineUp` / `ScrollLineDown`
- `ScrollToTop` / `ScrollToBottom`
- `ClearHistory` — drop the scrollback buffer (does not clear the visible
  screen; pair with `chars = "\x0c"` on a separate binding if you also want a
  `Ctrl+L`-style screen clear).

### Window / sessions

- `SpawnNewInstance` / `CreateNewWindow` / `CreateNewTab` — all three open a
  new shell session in the current workspace. (Alacritty distinguishes
  windows from tabs; alacritree has a single window with sessions per
  workspace, so they collapse to the same action.)
- `SelectNextTab` / `SelectPreviousTab` — cycle through sessions in the
  current workspace.
- `SelectTab1` … `SelectTab9` — select the Nth session in the current
  workspace. Out-of-range indices are ignored.
- `SelectLastTab` — select the last session in the current workspace.
- `ToggleFullscreen`
- `ToggleMaximized`
- `Minimize`
- `Quit` — open the quit confirmation dialog.

### Misc

- `None` — consume the key without doing anything. Useful to unbind a
  default shortcut.

---

## Not supported

These alacritty actions exist but are intentionally not wired up:

- **Vi mode** (`ToggleViMode`, all `ViAction`/`ViMotion` variants) — alacritree
  does not track terminal modes, so any binding gated by `mode = "Vi"` or
  `mode = "~Vi"` is dropped at parse time.
- **Search mode** (`SearchForward`, `SearchBackward`, all `SearchAction`
  variants) — no in-app search UI yet.
- **Hints** (`Hint(...)`) — regex hinting is an alacritty renderer feature.
- **Mouse-only actions** (`CopySelection`, `ClearSelection`,
  `ExpandSelection`) — depend on the selection model alacritree's grid does
  not have.
- **Platform-specific window actions** (`Hide`, `HideOtherApplications`,
  `ToggleSimpleFullscreen`) — alacritty calls into AppKit directly; eframe
  doesn't expose the equivalent.
- **`ClearLogNotice`** — alacritree has no in-app log notice.
- **`Command`** (`command = { ... }`) — spawning arbitrary external processes
  from a keybinding is a security surface we haven't designed for yet.

Bindings with these actions are still parsed; they just log at `debug` and
otherwise do nothing.

---

## Customizing

Add `[[keyboard.bindings]]` tables to `alacritty.toml` or `alacritree.toml`
under your config directory (typically `~/.config/alacritty/`). Both files are
deep-merged, so alacritree-specific overrides can live in `alacritree.toml`
without touching the alacritty config.

```toml
# Example: bind Ctrl+Shift+T to open a new session, and unbind Cmd+M on macOS.
[[keyboard.bindings]]
key = "T"
mods = "Control|Shift"
action = "CreateNewTab"

[[keyboard.bindings]]
key = "M"
mods = "Command"
action = "None"
```

Modifier names: `Control` / `Ctrl`, `Shift`, `Alt` / `Option`, `Super` /
`Command` / `Meta`. Combine with `|`.

For raw byte sequences, use `chars = "..."` instead of `action`:

```toml
[[keyboard.bindings]]
key = "Return"
mods = "Alt"
chars = "\r"   # ESC + CR — meta-Enter for tmux prefix, etc.
```
