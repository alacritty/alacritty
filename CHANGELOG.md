# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Fix color issue in ncurses programs by updating terminfo pairs from 0x10000 to 0x7FFF
- Fix panic after quitting Alacritty on macOS
- Tabs are no longer replaced by spaces when copying them to the clipboard

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

### Removed

- The `custom_cursor_colors` config field was deleted, remove the `colors.cursor.*` options
  to achieve the same behavior as setting it to `false`
- The `scale_with_dpi` configuration value has been removed, on Linux the env
    variable `WINIT_HIDPI_FACTOR=1` can be set instead to disable DPI scaling

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

### Removed

- The terminfo entry `alacritty-256color`. It is replaced by the `alacritty`
  entry (which also advertises 256 colors)

### Fixed

- Rendering now occurs without the terminal locked which improves performance
- Clear screen properly before rendering of content to prevent various graphical glitches
- Fix build failure on 32-bit systems
- Windows started as unfocused now show the hollow cursor if the setting is enabled
- Empty lines in selections are now properly copied to the clipboard
- Selection start point lagging behind initial cursor position
- Rendering of selections which start above the visible area and end below it

### Deprecated

- The config option `window.decorations` should now use `full` or `none` instead
  of `true` or `false`, respectively.

### Security

- Bracketed paste mode now filters escape sequences beginning with \x1b

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

### Changed

- Multiple key/mouse bindings for a single key will now all be executed instead of picking one and
  ignoring the rest
- Improve text scrolling performance (affects applications like `yes`, not scrolling the history)

### Fixed

- Clear the visible region when the RIS escape sequence (`echo -ne '\033c'`) is received
- Prevent logger from crashing Alacritty when stdout/stderr is not available
- Fix a crash when sending the IL escape sequence with a large number of lines
