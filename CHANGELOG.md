# Changelog

All notable changes to Alacritty are documented in this file.
The sections should follow the order `Packaging`, `Added`, `Changed`, `Fixed` and `Removed`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

Notable changes to the `alacritty_terminal` crate are documented in its
[CHANGELOG](./alacritty_terminal/CHANGELOG.md).

## 0.17.0-dev

### Packaging

- Fixed invalid logo SVG attributes

### Changed

- Don't highlight hints on hover when the mouse cursor is hidden
- Require explicit tap to enable IME with touch input

### Fixed

- Brief error popup when saving the config file with some editors
- Subprocesses on OpenBSD now run with their CWD set to that of the shell's foreground process.

## 0.16.1

### Fixed

- Crashes on GPUs with partial robustness support

## 0.16.0

### Packaging

- Minimum Rust version has been bumped to 1.85.0

### Added

- Vi motions `*`, `#`, `{`, and `}`
- IPC config retrieval using `alacritty msg get-config`
- Multi-sequence touch zoom sequences
- Vi action `Y` keybind, yank to the end of line
- Add `/etc/alacritty/alacritty.toml` fallback for system wide configuration
- Unicode 17 support

### Changed

- Hide login message if `~/.hushlogin` is present
- Improve rendering of rounded corners with builtin box drawing

### Fixed

- Crash when OpenGL context resets
- Modifier keys clearing selection with kitty keyboard protocol enabled
- `glyph_offset.y` not applied to strikeout
- `Enter`,`Tab`, `Backspace` not disambiguated with `shift` in kitty keyboard's disambiguate mode
- Hint bindings not respecting IPC overrides
- Search matching a wrapping fullwidth character in the last column
- Crash when `AppleFontSmoothing` option is not present on macOS
- Origin mode (DECOM) not moving cursor to the origin point
- Unresponsiveness when spamming the bell character with a bell command enabled
- `window.startup_mode` applied to existing window when opening a new tab on macOS
- Slowdowns over time on macOS 26

## 0.15.1

### Changed

- Error out when socket fails to create with `--daemon`
- Default URL hints now stop before backslashes

### Fixed

- Modifiers being out of sync for fast/synthetic input on X11
- Child process creation failing while inside a deleted directory
- Shifted key reported without a shift when using kitty keyboard protocol

## 0.15.0

### Added

- Config option `window.level = "AlwaysOnTop"` to force Alacritty to always be the toplevel window
- Escape sequence to move cursor forward tabs ( CSI Ps I )
- Pass activation token in `alacritty msg create-window` on Wayland/X11

### Changed

- Always focus new windows on macOS
- Don't switch to semantic/line selection when control is pressed
- Always emit `1` for the first parameter when having modifiers in kitty keyboard protocol

### Fixed

- Mouse/Vi cursor hint highlighting broken on the terminal cursor line
- Hint launcher opening arbitrary text, when terminal content changed while opening
- `SemanticRight`/`SemanticLeft` vi motions breaking with wide semantic escape characters
- `alacritty migrate` crashing with recursive toml imports
- Migrating nonexistent toml import breaking the entire migration
- First daemon mode window ignoring window options passed through CLI
- Report of Enter/Tab/Backspace in kitty keyboard's report event types mode
- Crash when pressing certain modifier keys on macOS 15+
- Cut off wide characters in preedit string

## 0.14.0

### Packaging

- Minimum Rust version has been bumped to 1.74.0

### Added

- Support relative path imports from config files
- `alacritty migrate` support for TOML configuration changes
- Headless mode using `alacritty --daemon`

### Changed

- Pressing `Alt` with unicode input will now add `ESC` like for ASCII input
- Decorations use opaque style and system window background on macOS
- No longer source `~/.zshenv` on macOS
- Moved config options `import`, `working_directory`, `live_config_reload`, and `ipc_socket`
    to the new `general` section
- Moved config option `shell` to `terminal.shell`
- `ctrl+shift+u` binding to open links to `ctrl+shift+o` to avoid collisions with IMEs
- Use `Beam` cursor for single char cursor inside the IME preview

### Fixed

- Crash when trying to create a new tab without decorations enabled
- New window being treated as focused when it's not on Wayland
- IME preview blending into text below it
- Dynamic title disabled for new windows when initial one has title as CLI option
- While terminal in mouse mode, mouse bindings that used the shift modifier and
  had multiple actions only performed the first action
- Leaking FDs when closing windows on Unix systems
- Config emitting errors for nonexistent import paths
- Kitty keyboard protocol reporting shifted key codes
- Broken search with words broken across line boundary on the first character
- Config import changes not being live reloaded
- Cursor color requests with default cursor colors
- Fullwidth semantic escape characters
- Windows app icon now displays properly in old alt+tab on Windows
- Alacritty not being properly activated with startup notify
- Invalid URL highlights after terminal scrolling
- Hollow block cursor not spanning multiple chars being edited inside the IME preview
- Vi inline search only working for direct key input without modifiers

## 0.13.2

### Added

- Default `Home`/`End` bindings in Vi mode mapped to `First`/`Last` respectively

### Fixed

- CLI env variables clearing configuration file variables
- Vi inline search/semantic selection expanding across newlines
- C0 and C1 codes being emitted in associated text when using kitty keyboard
- Occasional hang on startup with some Wayland compositors
- Missing key for `NumpadDecimal` in key bindings
- Scrolling content upwards moving lines into history when it shouldn't
- Sticky keys not working sometimes on X11
- Modifiers occasionally getting desynced on X11
- Autokey no longer working with alacritty on X11
- Freeze when moving window between monitors on Xfwm
- Mouse cursor not changing on Wayland when cursor theme uses legacy cursor icon names
- Config keys are available under proper names
- Build failure when compiling with x11 feature on NetBSD
- Hint `Select` action selecting the entire line for URL escapes
- Kitty encoding used for regular keys when they don't carry text

### Changed

- No unused-key warnings will be emitted for OS-specific config keys
- Use built-in font for sextant symbols from `U+1FB00` to `U+1FB3B`
- Kitty encoding is not used anymore for uncommon keys unless the protocol enabled

## 0.13.1

### Added

- Support for pasting in Vi + Search mode

### Changed

- `alacritty migrate` will ignore null values in yaml instead of erroring out

### Fixed

- `alacritty migrate` failing with nonexistent imports
- `Alt` bindings requiring composed key rather than pre-composed one on macOS
- `Alt + Control` bindings not working on Windows
- `chars = "\u000A"` action in bindings inserting `\n`
- Alternate keys not sent for `Shift + <number>` when using kitty protocol
- Alternative keys being swapped in kitty protocol implementation
- Powerline glyphs being cut for narrow fonts
- Xmodmap not working on X11
- Occasional slow startup on some X11 window managers
- Blurry window when using `window.dimensions` on some Wayland compositors
- IME input lagging behind on X11
- xdotool modifiers input not working correctly on X11
- Parsing numbers fails for mouse bindings
- Some config options overriding each other in CLI/IPC
- Numpad `Left` used for numpad `Up`

## 0.13.0

### Packaging

- Minimum Rust version has been bumped to 1.70.0
- Manpages are now generated using `scdoc` (see `INSTALL.md`)

### Added

- Warnings for unused configuration file options
- Config option `persist` in `hints` config section
- Support for dynamically loading conpty.dll on Windows
- Support for keybindings with dead keys
- `Back`/`Forward` mouse buttons support in bindings
- Copy global IPC options (`-w -1`) for new windows
- Bindings to create and navigate tabs on macOS
- Support startup notify protocol to raise initial window on Wayland/X11
- Debug option `prefer_egl` to prioritize EGL over other display APIs
- Inline vi-mode search using `f`/`F`/`t`/`T`
- `window.blur` config option to request blur for transparent windows
- `--option` argument for `alacritty msg create-window`
- Support for `DECRQM`/`DECRPM` escape sequences
- Support for kitty's keyboard protocol

### Changed

- Mode-specific bindings can now be bound in any mode for easier macros
- `--help` output is more compact now and uses more neutral palette
- Configuration file now uses TOML instead of YAML
    Run `alacritty migrate` to automatically convert all configuration files
- Deprecated config option `draw_bold_text_with_bright_colors`, use
    `colors.draw_bold_text_with_bright_colors`
- Deprecated config option `key_bindings`, use `keyboard.bindings`
- Deprecated config option `mouse_bindings`, use `mouse.bindings`
- The default colorscheme is now based on base16 classic dark
- IME popup now tries to not obscure the current cursor line
- The double click threshold was raised to `400ms`
- OSC 52 paste ability is now **disabled by default**; use `terminal.osc52` to adjust it
- Apply `colors.transparent_background_colors` for selections, hints, and search matches
- Underline full hint during keyboard selection
- Synchronized updates now use `CSI 2026` instead of legacy `DCS` variant
- In mouse mode with `Shift` pressed, mouse bindings without `Shift` are only triggered
    if no exact binding (i.e. one with `Shift`) is found.
- Use built-in font for powerline symbols from `U+E0B0` to `U+E0B3`
- Default `bell.animation` is now `Linear`
- `IncreaseFontSize/DecreaseFontSize` step is now 1px
- `font.size` precision was raised to 6 floating point digits
- Default font size to `11.25` matching 15px
- `Xft.dpi` is now reloaded when xsettingd change its value on X11

### Fixed

- Unconditional query of xdg-portal settings on Wayland
- `Maximized` startup mode not filling the screen properly on GNOME Wayland
- `OptionAsAlt` with `OnlyLeft`/`OnlyRight` settings not working properly on macOS
- Default Vi key bindings for `Last`/`First` actions not working on X11/Wayland
- Cut off wide characters in preedit string
- Scrolling on touchscreens
- Double clicking on CSD titlebar not always maximizing a window on Wayland
- Excessive memory usage when using regexes with a large number of possible states
- `window.decorations_theme_variant` not live reloading
- Copy/Paste being truncated to 64KiB on Wayland
- X11 clipboard lagging behind sometimes
- High wakeup count on Wayland due to clipboard polling
- Blocking paste freezing alacritty on Wayland
- `Command` modifier persisting after `Cmd + Tab` on macOS
- Crash on exit when using NVIDIA binary drivers on Wayland
- `window.startup_mode` applied to window again when creating new tab
- Crash when leaving search after resize
- Cursor being hidden after reaching cursor blinking timeout
- Message bar content getting stuck after closing with multiple messages on Wayland
- Vi cursor position not redrawn on PageUp/PageDown without scrollback
- Cursor not updating when blinking and viewport is scrolled
- Failure to start with recent version of mesa's i915 driver
- Error when using `chars` inside the mouse bindings

### Removed

- Config option `background_opacity`, use `window.background_opacity`
- Config option `colors.search.bar`, use `colors.footer_bar` instead
- Config option `mouse.url`, use the `hints` config section
- Config options `mouse.double_click` and `mouse.triple_click`

## 0.12.3

### Fixed

- Crash on macOS Sonoma due to change in macOS resize handling
- Crash when Wayland compositor advertises `wl_compositor@v5` interface

## 0.12.2

### Fixed

- Hyperlink preview not being shown when the terminal has exactly 2 lines
- Crash on Windows when changing display scale factor
- Freeze with some drivers when using GLX
- Crash when shrinking the terminal scrolled into the history

## 0.12.1

### Fixed

- Character `;` inside the `URI` in `OSC 8` sequence breaking the URI
- Selection on last line not updating correctly on resize
- Keyboard input not working on macOS with some IMEs like Fig.io
- Very long startup times on Wayland systems with broken xdg-portal setup.
- Error on startup with `GLX` when using old mesa platforms

## 0.12.0

### Added

- Uppercase `-T` short form for `--title`
- Support for horizontal scrolling in mouse mode and alternative scrolling modes
- Support for fractional scaling on Wayland with wp-fractional-scale protocol
- Support for running on GLES context
- Touchscreen input for click/scroll/select/zoom
- `window.resize_increments` config option, disabled by default

### Changed

- Always use sRGB color space on macOS
- Erase in line after the last column will no longer clear the last column
- Open new windows by default with macOS `Cmd`+`N` binding
- The hint about window transparency is now properly issued on Wayland and macOS
- `window.decorations_theme_variant` could now control theme on macOS and Windows
- The IME purpose is now set to `Terminal` which could help with OSK
- `window.decorations_theme_variant` is now using `Dark`, `Light`, and `None` values
- Resize increments are now set on macOS and X11 to resize by cell sizes

### Fixed

- `--help` output for `--class` does not match man pages
- Cursor and underlines always being black on very old hardware
- Crash when using very low negative `font.offset`
- Startup failure on macOS with default config when system `/bin/sh` is `dash`
- Artifacts in corners for maximized window with CSD on Wayland
- Dotted underline not shown on macOS
- Underline on macOS always being at the bottom of the cell
- Crash with `OT-SVG` fonts on Linux/BSD
- Crash during text compose on old GNOME under Wayland
- Mouse cursor staying hidden after window regains focus on macOS Ventura
- Blurry fonts when changing padding size at runtime
- Crash while typing on Wayland
- Multi-line semantic bracket selection
- Reduced GPU memory usage
- Low frame rate when multiple windows render at the same time
- Redraw hanging until a keypress on X11 in rare cases
- Window clipping when maximizing a window without decorations on Windows
- Quadrants not aligned with half blocks with built-in font
- EOT (`\x03`) escaping bracketed paste mode
- Drag & Drop not working for the search bar
- Simple-fullscreened window not resized when moving between monitors on macOS

### Removed

- `window.gtk_theme_variant` config field; use `window.decorations_theme_variant` instead
- `alt_send_esc` is now always set to `true`

## 0.11.0

### Packaging

- Minimum Rust version has been bumped to 1.60.0

## 0.11.0

### Packaging

- Minimum Rust version has been bumped to 1.57.0
- Renamed `io.alacritty.Alacritty.appdata.xml` to `org.alacritty.Alacritty.appdata.xml`
- Renamed `io.alacritty` to `org.alacritty` for `Alacritty.app`

### Added

- Track and report surface damage information to Wayland compositors
- Escape sequence for undercurl, dotted and dashed underlines (`CSI 4 : [3-5] m`)
- `ToggleMaximized` key binding action to (un-)maximize the active window, not bound by default
- Support for OpenGL ES 2.0
- Escape sequence to set underline color (`CSI 58 : 2 : Ps : Ps : Ps m`/`CSI 58 : 5 : Ps m`)
- Escape sequence to reset underline color (`CSI 59 m`)
- Vi mode keybinding (z) to center view around vi mode cursor
- Accept hexadecimal values starting with `0x` for `--embed`
- Config option `cursor.blink_timeout` to timeout cursor blinking after inactivity
- Escape sequence to set hyperlinks (`OSC 8 ; params ; URI ST`)
- Config `hints.enabled.hyperlinks` for hyperlink escape sequence hint highlight
- `window.decorations_theme_variant` to control both Wayland CSD and GTK theme variant on X11
- Support for inline input method

### Changed

- No longer renders to macos and x11 windows that are fully occluded / not directly visible
- The `--help` output was reworked with a new colorful syntax
- OSC 52 is now disabled on unfocused windows
- `SpawnNewInstance` no longer inherits initial `--command`
- Blinking cursor will timeout after `5` seconds by default
- Deprecated `colors.search.bar`, use `colors.footer_bar` instead
- On macOS, Alacritty now reads `AppleFontSmoothing` from user defaults to control font smoothing
- Warn when either `columns` or `lines` is non-zero, but not both
- Client side decorations should have proper text rendering now on Wayland
- Config option `window.gtk_theme_variant`, you should use `window.decorations_theme_variant` instead
- `--class` now sets both class part of WM_CLASS property and instance
- `--class`'s `general` and `instance` options were swapped
- Search bar is now respecting cursor thickness
- On X11 the IME popup window is stuck at the bottom of the window due to Xlib limitations
- IME no longer works in Vi mode when moving around

### Fixed

- Creating the IPC socket failing if `WAYLAND_DISPLAY` contains an absolute path
- Crash when resetting the terminal while in vi mode
- `font.glyph_offset` not live reloading
- Failure when running on 10-bit color system
- The colors being slightly different when using srgb displays on macOS
- Vi cursor blinking not reset when navigating in search
- Scrolling and middle-clicking modifying the primary selection
- Bottom gap for certain builtin box drawing characters
- Incorrect built-in glyphs for `U+2567` and `U+2568`
- Character mappings in the DEC special graphics character set (line drawing)
- Window flickering on resize on Wayland
- Unnecessary config reload when using `/dev/null` as a config file
- Windows `Open Alacritty Here` on root of drive displaying error
- On macOS, `font.use_thin_strokes` did not work since Big Sur
- On macOS, trying to load a disabled font would crash
- On macOS, Alacritty sessions did not appear in the list of tty sessions for `w` and `who`
- Cursor not hiding on GNOME Wayland
- Font having different scale factor after monitor powering off/on on X11
- Viewport not updating after opening a new tabbed window on macOS
- Terminal not exiting sometimes after closing all windows on macOS
- CPU usage spikes due to mouse movements for unfocused windows on X11/Windows
- First window on macOS not tabbed with system prefer tabs setting
- Window being treated as focused by default on Wayland

### Removed

- `font.use_thin_strokes` config field; to use thin strokes on macOS, set
    `AppleFontSmoothing` to 0 with `$ defaults write -g AppleFontSmoothing -int 0`

## 0.10.1

### Added

- Option `font.builtin_box_drawing` to disable the built-in font for drawing box characters

### Changed

- Builtin font thickness is now based on cell width instead of underline thickness

### Fixed

- OSC 4 not handling `?`
- `?` in OSC strings reporting default colors instead of modified ones
- OSC 104 not clearing colors when second parameter is empty
- Builtin font lines not contiguous when `font.offset` is used
- `font.glyph_offset` is no longer applied on builtin font
- Built-in font arcs alignment
- Repeated permission prompts on M1 macs
- Colors being slightly off when using `colors.transparent_background_colors`

## 0.10.0

### Packaging

- New `extra/alacritty-msg.man` manpage for the `alacritty msg` subcommand
- Minimum Rust version has been bumped to 1.53.0

### Added

- Option `colors.transparent_background_colors` to allow applying opacity to all background colors
- Support for running multiple windows from a single Alacritty instance (see docs/features.md)
- Urgency support on Wayland via `xdg_activation_v1`

### Changed

- `ExpandSelection` is now a configurable mouse binding action
- Config option `background_opacity`, you should use `window.opacity` instead
- Reload configuration files when their symbolic link is replaced
- Strip trailing whitespaces when yanking from a block selection
- Display area keeps history position when viewport is cleared
- Commands spawn from the current directory of the foreground shell in Unix-like systems
- Remove trailing newline from strings taken from hints or simple/semantic selections
- Builtin font is now used for box drawing characters from `U+2500` to `U+259f`
- Logs now print the time since startup instead of the local time

### Fixed

- Line indicator obstructing vi mode cursor when scrolled into history
- Vi mode search starting in the line below the vi cursor
- Invisible cursor with matching foreground/background colors
- Crash when hovering over a match emptied by post-processing
- Crash when the vi cursor is on the scrollback and viewport clear is invoked
- Freeze when the vi cursor is on the scrollback and scrollback clear is invoked
- Vi cursor on topmost of the display moving downward when scrolled into history with active output
- Input lag on Wayland with Nvidia binary driver
- Crash when hovering the mouse over fullwidth characters
- Do not create logfile if the file already exists

### Removed

- Wayland client side decorations were simplified

## 0.9.0

### Packaging

- Minimum Rust version has been bumped to 1.46.0

### Added

- Support for `ipfs`/`ipns` URLs
- Mode field for regex hint bindings

### Fixed

- Regression in rendering performance with dense grids since 0.6.0
- Crash/Freezes with partially visible fullwidth characters due to alt screen resize
- Incorrect vi cursor position after invoking `ScrollPage*` action
- Slow PTY read performance with extremely dense grids
- Crash when resizing during vi mode
- Unintentional text selection range change after leaving vi mode
- Deadlock on Windows during high frequency output
- Search without vi mode not starting at the correct location when scrolled into history
- Crash when starting a vi mode search from the bottommost line
- Original scroll position not restored after canceling search
- Clipboard copy skipping non-empty cells when encountering an interrupted tab character
- Vi mode cursor moving downward when scrolled in history with active output
- Crash when moving fullwidth characters off the side of the terminal in insert mode
- Broken bitmap font rendering with FreeType 2.11+
- Crash with non-utf8 font paths on Linux
- Newly installed fonts not rendering until Alacritty restart

## 0.8.0

### Packaging

- Minimum Rust version has been bumped to 1.45.0

### Packaging

- Updated shell completions
- Added ARM executable to prebuilt macOS binaries

### Added

- IME composition preview not appearing on Windows
- Synchronized terminal updates using `DCS = 1 s ST`/`DCS = 2 s ST`
- Regex terminal hints ([see features.md](./docs/features.md#hints))
- macOS keybinding (cmd+alt+H) hiding all windows other than Alacritty
- Support for `magnet` URLs

### Changed

- The vi mode cursor is now created in the top-left if the terminal cursor is invisible
- Focused search match will use cell instead of match colors for CellForeground/CellBackground
- URL highlighting has moved from `mouse.url` to the `hints` config section

### Fixed

- Alacritty failing to start on X11 with invalid DPI reported by XRandr
- Text selected after search without any match
- Incorrect vi cursor position after leaving search
- Clicking on URLs on Windows incorrectly opens File Explorer
- Incorrect underline cursor thickness on wide cell
- Viewport moving around when resizing while scrolled into history
- Block cursor not expanding across fullwidth characters when on the right side of it
- Overwriting fullwidth characters only clearing one of the involved cells

### Removed

- Config field `visual_bell`, you should use `bell` instead

## 0.7.2

### Packaging

- Updated shell completions

### Fixed

- Crash due to assertion failure on 32-bit architectures
- Segmentation fault on shutdown with Wayland
- Incorrect estimated DPR with Wayland
- Consecutive clipboard stores dropped on Wayland until the application is refocused

## 0.7.1

### Fixed

- Jumping between matches in backward vi search

## 0.7.0

### Added

- Support for `~/` at the beginning of configuration file imports
- New `cursor.style.blinking` option to set the default blinking state
- New `cursor.blink_interval` option to configure the blinking frequency
- Support for cursor blinking escapes (`CSI ? 12 h`, `CSI ? 12 l` and `CSI Ps SP q`)
- IME support on Windows
- Urgency support on Windows
- Customizable keybindings for search
- History for search mode, bound to ^P/^N/Up/Down by default
- Default binding to cancel search on Ctrl+C
- History position indicator for search and vi mode

### Changed

- Nonexistent config imports are ignored instead of raising an error
- Value for disabling logging with `config.log_level` is `Off` instead of `None`
- Missing glyph symbols are no longer drawn for zerowidth characters

### Fixed

- Wide characters sometimes being cut off
- Preserve vi mode across terminal `reset`
- Escapes `CSI Ps b` and `CSI Ps Z` with large parameters locking up Alacritty
- Dimming colors which use the indexed `CSI 38 : 5 : Ps m` notation
- Slow rendering performance with a lot of cells with underline/strikeout attributes
- Performance of scrolling regions with offset from the bottom
- Extra mouse buttons are no longer ignored on Wayland
- Numpad arrow keys are now properly recognized on Wayland
- Compilation when targeting aarch64-apple-darwin
- Window not being completely opaque on Windows
- Window being always on top during alt-tab on Windows
- Cursor position not reported to apps when mouse is moved with button held outside of window
- No live config update when starting Alacritty with a broken configuration file
- PTY not drained to the end with the `--hold` flag enabled
- High CPU usage on BSD with live config reload enabled
- Alacritty not discarding invalid escape sequences starting with ESC
- Crash due to clipboard not being properly released on Wayland
- Shadow artifacts when resizing transparent windows on macOS
- Missing glyph symbols not being rendered for missing glyphs on macOS and Windows
- Underline cursor being obscured by underline
- Cursor not being rendered with a lot of unicode glyphs visible
- IME input swallowed after triggering a key binding
- Crash on Wayland due to non-standard fontconfig configuration
- Search without vi mode not jumping properly between all matches

### Removed

- The following CLI arguments have been removed in favor of the `--option` flag:
    * `--persistent-logging`
    * `--live-config-reload`
    * `--no-live-config-reload`
    * `--dimensions`
    * `--position`
- `live-shader-reload` feature
- Config option `dynamic_title`, you should use `window.dynamic_title` instead
- Config option `scrolling.faux_multiplier`, which was replaced by escape `CSI ? 1007 h/l`
- WinPTY support on Windows

## 0.6.0

### Packaging

- Minimum Rust version has been bumped to 1.43.0
- The snapcraft.yaml file has been removed
- Updated `setab`/`setaf` capabilities in `alacritty-direct` to use colons
- WinPTY is now enabled only when targeting MSVC
- Deprecated the WinPTY backend feature, disabling it by default

### Added

- Secondary device attributes escape (`CSI > 0 c`)
- Support for colon separated SGR 38/48
- New Ctrl+C binding to cancel search and leave vi mode
- Escapes for double underlines (`CSI 4 : 2 m`) and underline reset (`CSI 4 : 0 m`)
- Configuration file option for sourcing other files (`import`)
- CLI parameter `--option`/`-o` to override any configuration field
- Escape sequences to report text area size in pixels (`CSI 14 t`) and in characters (`CSI 18 t`)
- Support for single line terminals dimensions
- Right clicking on Wayland's client side decorations will show application menu
- Escape sequences to enable and disable window urgency hints (`CSI ? 1042 h`, `CSI ? 1042 l`)

### Changed

- Cursors are now inverted when their fixed color is similar to the cell's background
- Use the working directory of the terminal foreground process, instead of the shell's working
    directory, for `SpawnNewInstance` action
- Fallback to normal underline for unsupported underline types in `CSI 4 : ? m` escapes
- The user's background color is now used as the foreground for the render timer
- Use yellow/red from the config for error and warning messages instead of fixed colors
- Existing CLI parameters are now passed to instances spawned using `SpawnNewInstance`
- Wayland's Client side decorations now use the search bar colors
- Reduce memory usage by up to at least 30% with a full scrollback buffer
- The number of zerowidth characters per cell is no longer limited to 5
- `SpawnNewInstance` is now using the working directory of the terminal foreground process on macOS

### Fixed

- Incorrect window location with negative `window.position` config options
- Slow rendering performance with HiDPI displays, especially on macOS
- Keys swallowed during search when pressing them right before releasing backspace
- Crash when a wrapped line is rotated into the last line
- Selection wrapping to the top when selecting below the error/warning bar
- Pasting into clients only supporting `UTF8_STRING` mime type on Wayland
- Crash when copying/pasting with neither pointer nor keyboard focus on Wayland
- Crash due to fd leak on Wayland
- IME window position with fullwidth characters in the search bar
- Selection expanding over 2 characters when scrolled in history with fullwidth characters in use
- Selection scrolling not starting when mouse is over the message bar
- Incorrect text width calculation in message bar when the message contains multibyte characters
- Remapped caps lock to escape not triggering escape bindings on Wayland
- Crash when setting overly long title on Wayland
- Switching in and out of various window states, like Fullscreen, not persisting window size on Wayland
- Crash when providing 0 for `XCURSOR_SIZE` on Wayland
- Gap between window and server side decorations on KWIN Wayland
- Wayland's client side decorations not working after tty switch
- `Fullscreen` startup mode not working on Wayland
- Window not being rescaled when changing DPR of the current monitor on Wayland
- Crash in some cases when pointer isn't presented upon startup on Wayland
- IME not working on Wayland
- Crash on startup on GNOME since its 3.37.90 version on Wayland
- Touchpad scrolling scrolled less than it should on macOS/Wayland on scaled outputs
- Incorrect modifiers at startup on X11
- `Add` and `Subtract` keys are now named `NumpadAdd` and `NumpadSubtract` respectively
- Feature checking when cross compiling between different operating systems
- Crash when writing to the clipboard fails on Wayland
- Crash with large negative `font.offset.x/y`
- Visual bell getting stuck on the first frame
- Zerowidth characters in the last column of the line

## 0.5.0

### Packaging

- Minimum Rust version has been bumped to 1.41.0
- Prebuilt Linux binaries have been removed
- Added manpage, terminfo, and completions to macOS application bundle
- On Linux/BSD the build will fail without Fontconfig installed, instead of building it from source
- Minimum FreeType version has been bumped to 2.8 on Linux/BSD

### Added

- Default Command+N keybinding for SpawnNewInstance on macOS
- Vi mode for regex search, copying text, and opening links
- `CopySelection` action which copies into selection buffer on Linux/BSD
- Option `cursor.thickness` to set terminal cursor thickness
- Font fallback on Windows
- Support for Fontconfig embolden and matrix options
- Opt-out compilation flag `winpty` to disable WinPTY support
- Scrolling during selection when mouse is at top/bottom of window
- Expanding existing selections using single, double and triple click with the right mouse button
- Support for `gopher` and `gemini` URLs
- Unicode 13 support
- Option to run command on bell which can be set in `bell.command`
- Fallback to program specified in `$SHELL` variable on Linux/BSD if it is present
- Ability to make selections while search is active

### Changed

- Block cursor is no longer inverted at the start/end of a selection
- Preserve selection on non-LMB or mouse mode clicks
- Wayland client side decorations are now based on config colorscheme
- Low resolution window decoration icon on Windows
- Mouse bindings for additional buttons need to be specified as a number not a string
- Don't hide cursor on modifier press with `mouse.hide_when_typing` enabled
- `Shift + Backspace` now sends `^?` instead of `^H`
- Default color scheme is now `Tomorrow Night` with the bright colors of `Tomorrow Night Bright`
- Set IUTF8 termios flag for improved UTF8 input support
- Dragging files into terminal now adds a space after each path
- Default binding replacement conditions
- Adjusted selection clearing granularity to more accurately match content
- To use the cell's text color for selection with a modified background, the `color.selection.text`
    variable must now be set to `CellForeground` instead of omitting it
- URLs are no longer highlighted without a clearly delimited scheme
- Renamed config option `visual_bell` to `bell`
- Moved config option `dynamic_title` to `window.dynamic_title`
- When searching without vi mode, matches are only selected once search is cancelled

### Fixed

- Selection not cleared when switching between main and alt grid
- Freeze when application is invisible on Wayland
- Paste from some apps on Wayland
- Slow startup with Nvidia binary drivers on some X11 systems
- Display not scrolling when printing new lines while scrolled in history
- Regression in font rendering on macOS
- Scroll down escape (`CSI Ps T`) incorrectly pulling lines from history
- Dim escape (`CSI 2 m`) support for truecolor text
- Incorrectly deleted lines when increasing width with a prompt wrapped using spaces
- Documentation for class in `--help` missing information on setting general class
- Linewrap tracking when switching between primary and alternate screen buffer
- Preservation of the alternate screen's saved cursor when swapping to primary screen and back
- Reflow of cursor during resize
- Cursor color escape ignored when its color is set to inverted in the config
- Fontconfig's `autohint` and `hinting` options being ignored
- Ignoring of default FreeType properties
- Alacritty crashing at startup when the configured font does not exist
- Font size rounding error
- Opening URLs while search is active

### Removed

- Environment variable `RUST_LOG` for selecting the log level
- Deprecated `window.start_maximized` config field
- Deprecated `render_timer` config field
- Deprecated `persistent_logging` config field

## 0.4.3

### Fixed

- Tabstops not being reset with `reset`
- Fallback to `LC_CTYPE=UTF-8` on macOS without valid system locale
- Resize lag on launch under some X11 wms
- Increased input latency due to vsync behavior on X11
- Emoji colors blending with terminal background
- Fix escapes prematurely terminated by terminators in unicode glyphs
- Incorrect location when clicking inside an unfocused window on macOS
- Startup mode `Maximized` on Windows
- Crash when writing a fullwidth character in the last column with auto-wrap mode disabled
- Crashing at startup on Windows

## 0.4.2

### Packaging

- Minimum Rust version has been bumped to 1.37.0
- Added Rust features `x11` and `wayland` to pick backends, with both enabled by default
- Capitalized the Alacritty.desktop file

### Added

- Live config reload for `window.title`

### Changed

- Pressing additional modifiers for mouse bindings will no longer trigger them
- Renamed `WINIT_HIDPI_FACTOR` environment variable to `WINIT_X11_SCALE_FACTOR`
- Print an error instead of crashing, when startup working directory is invalid
- Line selection will now expand across wrapped lines
- The default value for `draw_bold_text_with_bright_colors` is now `false`
- Mirror OSC query terminators instead of always using BEL
- Increased Beam, Underline, and Hollow Block cursors' line widths
- Dynamic title is not disabled anymore when `window.title` is set in config

### Fixed

- Incorrect default config path in `--help` on Windows and macOS
- Semantic selection stopping at full-width glyphs
- Full-width glyphs cut off in last column
- Crash when starting on some X11 systems
- Font size resetting when Alacritty is moved between screens
- Limited payload length in clipboard escape (used for Tmux copy/paste)
- Alacritty not ignoring keyboard events for changing WM focus on X11
- Regression which added a UNC path prefix to the working directory on Windows
- CLI parameters discarded when config is reload
- Blurred icons in KDE task switcher (alacritty.ico is now high-res)
- Consecutive builds failing on macOS due to preexisting `/Application` symlink
- Block selection starting from first column after beginning leaves the scrollback
- Incorrect selection status of the first cell when selection is off screen
- Backwards bracket selection
- Stack overflow when printing shader creation error
- Underline position for bitmap fonts
- Selection rotating outside of scrolling region
- Throughput performance problems caused by excessive font metric queries
- Unicode throughput performance on Linux/BSD
- Resize of bitmap fonts
- Crash when using bitmap font with `embeddedbitmap` set to `false`
- Inconsistent fontconfig fallback
- Handling of OpenType variable fonts
- Expansion of block-selection on partially selected full-width glyphs
- Minimize action only works with decorations on macOS
- Window permanently vanishing after hiding on macOS
- Handling of URLs with single quotes
- Parser reset between DCS escapes
- Parser stopping at unknown DEC private modes/SGR character attributes
- Block selection appending duplicate newlines when last column is selected
- Bitmap fonts being a bit smaller than they should be in some cases
- Config reload creating alternate screen history instead of updating scrollback
- Crash on Wayland compositors supporting `wl_seat` version 7+
- Message bar not hiding after fixing wrong color value in config
- Tabstops cleared on resize
- Tabstops not breaking across lines
- Crash when parsing DCS escape with more than 16 parameters
- Ignoring of slow touchpad scrolling
- Selection invisible when starting above viewport and ending below it
- Clipboard not working after TTY switch on Wayland
- Crash when pasting non UTF-8 string advertised as UTF-8 string on Wayland
- Incorrect modifiers tracking on X11 and macOS, leading to 'sticky' modifiers
- Crash when starting on Windows with missing dark mode support
- Variables `XCURSOR_THEME` and `XCURSOR_SIZE` ignored on Wayland
- Low resolution mouse cursor and decorations on HiDPI Wayland outputs
- Decorations visible when in fullscreen on Wayland
- Window size not persisted correctly after fullscreening on macOS
- Crash on startup with some locales on X11
- Shrinking terminal height in alt screen deleting primary screen content

### Removed

- Config option `auto_scroll`, which is now always disabled
- Config option `tabspaces`, which is now fixed at `8`

## 0.4.1

### Packaging

- Added compatibility logo variants for environments which can't render the default SVG

### Added

- Terminal escape bindings with combined modifiers for Delete and Insert
- /Applications symlink into OS X DMG for easier installation
- Colored emojis on Linux/BSD
- Value `randr` for `WINIT_HIDPI_FACTOR`, to ignore `Xft.dpi` and scale based on screen dimensions
- `Minimize` key binding action, bound to `cmd + m` on macOS

### Changed

- On Windows, the ConPTY backend will now be used by default if available
- The `enable_experimental_conpty_backend` config option has been replaced with `winpty_backend`

### Fixed

- URLs not truncated with non-matching single quote
- Absolute file URLs (`file:///home`) not recognized because of leading `/`
- Clipboard escape `OSC 52` not working with empty clipboard parameter
- Direct escape input on Windows using alt
- Incorrect window size on X11 when waking up from suspend
- Width of Unicode 11/12 emojis
- Minimize on windows causing layout issues
- Performance bottleneck when clearing colored rows
- Vague startup crash messages on Windows with WinPTY backend
- Deadlock on Windows when closing Alacritty using the title bar "X" button (ConPTY backend)
- Crash on `clear` when scrolled up in history
- Entire screen getting underlined/stroke out when running `clear`
- Slow startup on some Wayland compositors
- Padding not consistently visible on macOS
- Decorations ignoring Windows dark theme
- Crash on macOS when starting maximized without decorations
- Resize cursor not showing up on Wayland
- Maximized windows spawning behind system panel on Gnome Wayland

### Removed

- Support for 8-bit C1 escape sequences

## 0.4.0

### Packaging

- Minimum Rust version has been bumped to 1.36.0
- Config is not generated anymore, please consider distributing the alacritty.yml as documentation
- Removed Alacritty terminfo from .deb in favor of ncurses provided one

### Added

- Block selection mode when Control is held while starting a selection
- Allow setting general window class on X11 using CLI or config (`window.class.general`)
- Config option `window.gtk_theme_variant` to set GTK theme variant
- Completions for `--class` and `-t` (short title)
- Change the mouse cursor when hovering over the message bar and its close button
- Support combined bold and italic text (with `font.bold_italic` to customize it)
- Extra bindings for F13-F20
- Terminal escape bindings with combined modifiers
- Bindings for ScrollToTop and ScrollToBottom actions
- `ReceiveChar` key binding action to insert the key's text character
- New CLI flag `--hold` for keeping Alacritty opened after its child process exits
- Escape sequence to save and restore window title from stack
- Alternate scroll escape sequence (`CSI ? 1007 h` / `CSI ? 1007 l`)
- Print name of launch command if Alacritty failed to execute it
- Live reload font settings from config
- UTF-8 mouse mode escape sequence (`CSI ? 1005 h` / `CSI ? 1005 l`)
- Escape for reading clipboard (`OSC 52 ; <s / p / c> ; ? BEL`)
- Set selection clipboard (`OSC 52 ; <s / p> ; <BASE64> BEL`)

### Changed

- On Windows, query DirectWrite for recommended anti-aliasing settings
- Scroll lines out of the visible region instead of deleting them when clearing the screen

### Fixed

- GUI programs launched by Alacritty starting in the background on X11
- Text Cursor position when scrolling
- Performance issues while resizing Alacritty
- First unfullscreen action ignored on window launched in fullscreen mode
- The window is now filled with the background color before displaying
- Cells sometimes not getting cleared correctly
- X11 clipboard hanging when mime type is set
- On macOS, Alacritty will now fallback to Menlo if a font specified in the config cannot be loaded
- Debug ref tests are now written to disk regardless of shutdown method
- Cursor color setting with escape sequence
- Override default bindings with subset terminal mode match
- On Linux, respect fontconfig's `embeddedbitmap` configuration option
- Selecting trailing tab with semantic expansion
- URL parser incorrectly handling Markdown URLs and angled brackets
- Intermediate bytes of CSI sequences not checked
- Wayland clipboard integration
- Use text mouse cursor when mouse mode is temporarily disabled with shift
- Wayland primary selection clipboard not storing text when selection is stopped outside of the window
- Block URL highlight while a selection is active
- Bindings for Alt + F1-F12
- Discard scrolling region escape with bottom above top
- Opacity always applying to cells with their background color matching the terminal background
- Allow semicolons when setting titles using an OSC
- Background always opaque on X11
- Skipping redraws on PTY update
- Not redrawing while resizing on Windows/macOS
- Decorations `none` launching an invisible window on Windows
- Alacritty turning transparent when opening another window on macOS with chunkwm
- Startup mode `Maximized` having no effect on Windows
- Inserting Emojis using `Super+.` or compose sequences on Windows
- Change mouse cursor depending on mode with Wayland
- Hide mouse cursor when typing if the `mouse.hide_when_typing` option is set on Wayland
- Glitches when DPI changes on Windows
- Crash when resuming after suspension
- Crash when trying to start on X11 with a Wayland compositor running
- Crash with a virtual display connected on X11
- Use `\` instead of `\\` as path separators on Windows for logging config file location
- Underline/strikeout drawn above visual bell
- Terminal going transparent during visual bell
- Selection not being cleared when sending chars through a binding
- Mouse protocols/encodings not being mutually exclusive within themselves
- Escape `CSI Ps M` deleting lines above cursor when at the bottom of the viewport
- Cell reset not clearing underline, strikeout and foreground color
- Escape `CSI Ps c` honored with a wrong `Ps`
- Ignore `ESC` escapes with invalid intermediates
- Blank lines after each line when pasting from GTK apps on Wayland

### Removed

- Bindings for Super/Command + F1-F12
- Automatic config generation
- Deprecated `scrolling.faux_multiplier`, the alternate scroll escape can now be used to disable it
    and `scrolling.multiplier` controls the number of scrolled lines

## 0.3.3

### Packaging

- Add appstream metadata, located at /extra/linux/io.alacritty.Alacritty.xml
- The xclip dependency has been removed
- On macOS, Alacritty now requests NSSystemAdministrationUsageDescription to
   avoid permission failures
- Minimum Rust version has been bumped to 1.32.0

### Added

- Added ToggleFullscreen action
- On macOS, there's a ToggleSimpleFullscreen action which allows switching to
    fullscreen without occupying another space
- A new window option `window.startup_mode` which controls how the window is created
- `_NET_WM_ICON` property is set on X11 now, allowing for WMs to show icons in titlebars
- Current Git commit hash to `alacritty --version`
- Config options `window.title` and `window.class`
- Config option `working_directory`
- Config group `debug` with the options `debug.log_level`, `debug.print_events`
    and `debug.ref_test`
- Select until next matching bracket when double-clicking a bracket
- Added foreground/background escape code request sequences
- Escape sequences now support 1, 3, and 4 digit hex colors

### Changed

- On Windows, Alacritty will now use the native DirectWrite font API
- The `start_maximized` window option is now `startup_mode: Maximized`
- Cells with identical foreground and background will now show their text upon selection/inversion
- Default Window padding to 0x0
- Moved config option `render_timer` and `persistent_logging` to the `debug` group
- When the cursor is in the selection, it will be inverted again, making it visible

### Fixed

- Double-width characters in URLs only being highlighted on the left half
- PTY size not getting updated when message bar is shown
- Text Cursor disappearing
- Incorrect positioning of zero-width characters over double-width characters
- Mouse mode generating events when the cell has not changed
- Selections not automatically expanding across double-width characters
- On macOS, automatic graphics switching has been enabled again
- Text getting recognized as URLs without slashes separating the scheme
- URL parser dropping trailing slashes from valid URLs
- UTF-8 BOM skipped when reading config file
- Terminfo backspace escape sequence (`kbs`)

### Removed

- Deprecated `mouse.faux_scrollback_lines` config field
- Deprecated `custom_cursor_colors` config field
- Deprecated `hide_cursor_when_typing` config field
- Deprecated `cursor_style` config field
- Deprecated `unfocused_hollow_cursor` config field
- Deprecated `dimensions` config field

## Version 0.3.2

### Fixed

- Panic on startup when using Conpty on Windows

## Version 0.3.1

### Added

- Added ScrollLineUp and ScrollLineDown actions for scrolling line by line
- Native clipboard support on X11 and Wayland

### Changed

- Alacritty now has a fixed minimum supported Rust version of 1.31.0

### Fixed

- Reset scrolling region when the RIS escape sequence is received
- Subprocess spawning on macos
- Unnecessary resize at startup
- Text getting blurry after live-reloading shaders with padding active
- Resize events are not send to the shell anymore if dimensions haven't changed
- Minor performance issues with underline and strikeout checks
- Rare bug which would extend underline and strikeout beyond the end of line
- Cursors not spanning two lines when over double-width characters
- Incorrect cursor dimensions if the font offset isn't `0`

## Version 0.3.0

### Packaging

- On Linux, the .desktop file now uses `Alacritty` as icon name, which can be
    found at `extra/logo/alacritty-term.svg`

### Added

- MSI installer for Windows is now available
- New default key bindings Alt+Home, Alt+End, Alt+PageUp and Alt+PageDown
- Dynamic title support on Windows
- Ability to specify starting position with the `--position` flag
- New configuration field `window.position` allows specifying the starting position
- Added the ability to change the selection color
- Text will reflow instead of truncating when resizing Alacritty
- Underline text and change cursor when hovering over URLs with required modifiers pressed

### Changed

- Clicking on non-alphabetical characters in front of URLs will no longer open them
- Command keybindings on Windows will no longer open new cmd.exe console windows
- On macOS, automatic graphics switching has been temporarily disabled due to a macos bug

### Fixed

- Fix panic which could occur when quitting Alacritty on Windows if using the Conpty backend
- Automatic copying of selection to clipboard when mouse is released outside of Alacritty
- Scrollback history live reload only working when shrinking lines
- Crash when decreasing scrollback history in config while scrolled in history
- Resetting the terminal while in the alt screen will no longer disable scrollback
- Cursor jumping around when leaving alt screen while not in the alt screen
- Text lingering around when resetting while scrolled up in the history
- Terminfo support for extended capabilities
- Allow mouse presses and beginning of mouse selection in padding
- Windows: Conpty backend could close immediately on startup in certain situations
- FreeBSD: SpawnNewInstance will now open new instances in the shell's current
    working directory as long as linprocfs(5) is mounted on `/compat/linux/proc`
- Fix lingering Alacritty window after child process has exited
- Growing the terminal while scrolled up will no longer move the content down
- Support for alternate keyboard layouts on macOS
- Slow startup time on some X11 systems
- The AltGr key no longer sends escapes (like Alt)
- Fixes increase/decrease font-size keybindings on international keyboards
- On Wayland, the `--title` flag will set the Window title now
- Parsing issues with URLs starting in the first or ending in the last column
- URLs stopping at double-width characters
- Fix `start_maximized` option on X11
- Error when parsing URLs ending with Unicode outside of the ascii range
- On Windows, focusing a Window will no longer start a selection

## Version 0.2.9

### Changed

- Accept fonts which are smaller in width or height than a single pixel

### Fixed

- Incorrect font spacing after moving Alacritty between displays

## Version 0.2.8

### Added

- Window class on Wayland is set to `Alacritty` by default
- Log file location is stored in the `ALACRITTY_LOG` environment variable
- Close button has been added to the error/warning messages

### Changed

- Improve scrolling accuracy with devices sending fractional updates (like touchpads)
- `scrolling.multiplier` now affects normal scrolling with touchpads
- Error/Warning bar doesn't overwrite the terminal anymore
- Full error/warning messages are displayed
- Config error messages are automatically removed when the config is fixed
- Scroll history on Shift+PgUp/PgDown when scrollback history is available

### Fixed

- Resolved off-by-one issue with erasing characters in the last column
- Excessive polling every 100ms with `live_config_reload` enabled
- Unicode characters at the beginning of URLs are now properly ignored
- Remove error message when reloading an empty config
- Allow disabling URL launching by setting the value of `mouse.url.launcher` to `None`
- Corrected the `window.decorations` config documentation for macOS
- Fix IME position on HiDPI displays
- URLs not opening while terminal is scrolled
- Reliably remove log file when Alacritty is closed and persistent logging is disabled
- Remove selections when clearing the screen partially (scrolling horizontally in less)
- Crash/Freeze when shrinking the font size too far
- Documentation of the `--dimensions` flag have been updated to display the correct default

### Removed

- `clear` doesn't remove error/warning messages anymore

## Version 0.2.7

### Fixed

- Crash when trying to start Alacritty on Windows

## Version 0.2.6

### Added

- New `alt_send_esc` option for controlling if alt key should send escape sequences

### Changed

- All options in the configuration file are now optional

### Fixed

- Replaced `Command` with `Super` in the Linux and Windows config documentation
- Prevent semantic and line selection from starting with the right or middle mouse button
- Prevent Alacritty from crashing when started on a system without any free space
- Resolve issue with high CPU usage after moving Alacritty between displays
- Characters will no longer be deleted when using ncurses with the hard tab optimization
- Crash on non-linux operating systems when using the `SpawnNewInstance` action

### Removed

- Windows and macOS configuration files (`alacritty.yml` is now platform independent)

## Version 0.2.5

### Added

- New configuration field `visual_bell.color` allows changing the visual bell color
- Crashes on Windows are now also reported with a popup in addition to stderr
- Windows: New configuration field `enable_experimental_conpty_backend` which enables support
    for the Pseudoconsole API (ConPTY) added in Windows 10 October 2018 (1809) update
- New mouse and key action `SpawnNewInstance` for launching another instance of Alacritty

### Changed

- Log messages are now consistent in style, and some have been removed
- Windows configuration location has been moved from %USERPROFILE%\alacritty.yml
    to %APPDATA%\alacritty\alacritty.yml
- Windows default shell is now PowerShell instead of cmd
- URL schemes have been limited to http, https, mailto, news, file, git, ssh and ftp

### Fixed

- Fix color issue in ncurses programs by updating terminfo pairs from 0x10000 to 0x7FFF
- Fix panic after quitting Alacritty on macOS
- Tabs are no longer replaced by spaces when copying them to the clipboard
- Alt modifier is no longer sent separately from the modified key
- Various Windows issues, like color support and performance, through the new ConPTY
- Fixed rendering non default mouse cursors in terminal mouse mode (linux)
- Fix the `Copy` `mouse_bindings` action ([#1963](https://github.com/alacritty/alacritty/issues/1963))
- URLs are only launched when left-clicking
- Removal of extra characters (like `,`) at the end of URLs has been improved
- Single quotes (`'`) are removed from URLs when there is no matching opening quote
- Precompiled binaries now work with macOS versions before 10.13 (10.11 and above)

## Version 0.2.4

### Added

- Option for evenly spreading extra padding around the terminal (`window.dynamic_padding`)
- Option for maximizing alacritty on start (`window.start_maximized`)
- Display notice about errors and warnings inside Alacritty
- Log all messages to both stderr and a log file in the system's temporary directory
- New configuration option `persistent_logging` and CLI flag `--persistent-logging`,
    for keeping the log file after closing Alacritty
- `ClearLogNotice` action for removing the warning and error message
- Terminal bells on macOS will now request the user's attention in the window
- Alacritty now requests privacy permissions on macOS

### Changed

- Extra padding is not evenly spread around the terminal by default anymore
- When the config file is empty, Alacritty now logs an info instead of an error message

### Fixed

- Fixed a bad type conversion which could cause underflow on a window resize
- Alacritty now spawns a login shell on macOS, as with Terminal.app and iTerm2
- Fixed zombie processes sticking around after launching URLs
- Zero-width characters are now properly rendered without progressing the cursor

## Version 0.2.3

### Fixed

- Mouse cursor alignment issues and truncated last line caused by incorrect padding calculations

## Version 0.2.2

### Added

- Add support for Windows
- Add terminfo capabilities advertising support for changing the window title
- Allow using scancodes in the key_bindings section
- When `mouse.url.launcher` is set, clicking on URLs will now open them with the specified program
- New `mouse.url.modifiers` option to specify keyboard modifiers for opening URLs on click
- Binaries for macOS, Windows and Debian-based systems are now published with GitHub releases
- The keys F16-F24 have been added as options for key bindings
- DEB file adds Alacritty as option to `update-alternatives --config x-terminal-emulator`

### Changed

- The `colors.cursor.text` and `colors.cursor.cursor` fields are optional now
- Moved `cursor_style` to `cursor.style`
- Moved `unfocused_hollow_cursor` to `cursor.unfocused_hollow`
- Moved `hide_cursor_when_typing` to `mouse.hide_when_typing`
- Mouse bindings now ignore additional modifiers
- Extra padding is now spread evenly around the terminal grid
- DEB file installs to `usr/bin` instead of `usr/local/bin`

### Fixed

- Fixed erroneous results when using the `indexed_colors` config option
- Fixed rendering cursors other than rectangular with the RustType backend
- Selection memory leak and glitches in the alternate screen buffer
- Invalid default configuration on macOS and Linux
- Middle mouse pasting if mouse mode is enabled
- Selections now properly update as you scroll the scrollback buffer while selecting
- NUL character at the end of window titles
- DPI Scaling when moving windows across monitors
- On macOS, issues with Command-[KEY] and Control-Tab keybindings have been fixed
- Incorrect number of columns/lines when using the `window.dimensions` option
- On Wayland, windows will no longer be spawned outside of the visible region
- Resizing of windows without decorations
- On Wayland, key repetition works again
- On macOS, Alacritty will now use the integrated GPU again when available
- On Linux, the `WINIT_HIDPI_FACTOR` environment variable can be set from the config now

### Removed

- The `custom_cursor_colors` config field was deleted, remove the `colors.cursor.*` options
  to achieve the same behavior as setting it to `false`
- The `scale_with_dpi` configuration value has been removed, on Linux the env
    variable `WINIT_HIDPI_FACTOR=1` can be set instead to disable DPI scaling

## Version 0.2.1

### Added

- Implement the `hidden` escape sequence (`echo -e "\e[8mTEST"`)
- Add support for macOS systemwide dark mode
- Set the environment variable `COLORTERM="truecolor"` to advertise 24-bit color support
- On macOS, there are two new values for the config option `window.decorations`:
    - `transparent` - This makes the title bar transparent and allows the
        viewport to extend to the top of the window.
    - `buttonless` - Similar to transparent but also removed the buttons.
- Add support for changing the colors from 16 to 256 in the `indexed_colors` config section
- Add `save_to_clipboard` configuration option for copying selected text to the system clipboard
- New terminfo entry, `alacritty-direct`, that advertises 24-bit color support
- Add support for CSI sequences Cursor Next Line (`\e[nE`) and Cursor Previous Line (`\e[nF`)

### Changed

- Inverse/Selection color is now modelled after XTerm/VTE instead of URxvt to improve consistency
- First click on unfocused Alacritty windows is no longer ignored on platforms other than macOS
- Reduce memory usage significantly by only initializing part of the scrollback buffer at startup
- The `alacritty` terminfo entry no longer requires the `xterm` definition to be
  present on the system
- The default `TERM` value is no longer static; the `alacritty` entry is used if
  available, otherwise the `xterm-256color` entry is used instead
- The values `true` and `false` for the config option `window.decorations` have been replaced with
    `full` and `none`

### Fixed

- Rendering now occurs without the terminal locked which improves performance
- Clear screen properly before rendering of content to prevent various graphical glitches
- Fix build failure on 32-bit systems
- Windows started as unfocused now show the hollow cursor if the setting is enabled
- Empty lines in selections are now properly copied to the clipboard
- Selection start point lagging behind initial cursor position
- Rendering of selections which start above the visible area and end below it
- Bracketed paste mode now filters escape sequences beginning with \x1b

### Removed

- The terminfo entry `alacritty-256color`. It is replaced by the `alacritty`
  entry (which also advertises 256 colors)

## Version 0.2.0

### Added

- Add a scrollback history buffer (10_000 lines by default)
- CHANGELOG has been added for documenting relevant user-facing changes
- Add `ClearHistory` key binding action and the `Erase Saved Lines` control sequence
- When growing the window height, Alacritty will now try to load additional lines out of the
  scrollback history
- Support the dim foreground color (`echo -e '\033[2mDimmed Text'`)
- Add support for the LCD-V pixel mode (vertical screens)
- Pressing enter on the numpad should now insert a newline
- The mouse bindings now support keyboard modifiers (shift/ctrl/alt/super)
- Add support for the bright foreground color
- Support for setting foreground, background colors in one escape sequence

### Changed

- Multiple key/mouse bindings for a single key will now all be executed instead of picking one and
  ignoring the rest
- Improve text scrolling performance (affects applications like `yes`, not scrolling the history)

### Fixed

- Clear the visible region when the RIS escape sequence (`echo -ne '\033c'`) is received
- Prevent logger from crashing Alacritty when stdout/stderr is not available
- Fix a crash when sending the IL escape sequence with a large number of lines
