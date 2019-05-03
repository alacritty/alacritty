# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Packaging

- The xclip dependency has been removed

### Added

- Added ToggleFullscreen action
- On macOS, there's a ToggleSimpleFullscreen action which allows switching to
    fullscreen without occupying another space
- A new window option `startup_mode` which controls how the window is created
- `_NET_WM_ICON` property is set on X11 now, allowing for WMs to show icons in titlebars
- Current Git commit hash to `alacritty --version`

### Changed

- On Windows, Alacritty will now use the native DirectWrite font API
- The `start_maximized` window option is now `startup_mode: Maximized`
- Cells with identical foreground and background will now show their text upon selection/inversion

### Fixed

- Double-width characters in URLs only being highlit on the left half
- PTY size not getting updated when message bar is shown
- Text Cursor disappearing
- Incorrect positioning of zero-width characters over double-width characters
- Mouse mode generating events when the cell has not changed
- Selections not automatically expanding across double-width characters

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

### Removed

- Windows and macOS configuration files (`alacritty.yml` is now platform independent)

### Fixed

- Replaced `Command` with `Super` in the Linux and Windows config documentation
- Prevent semantic and line selection from starting with the right or middle mouse button
- Prevent Alacritty from crashing when started on a system without any free space
- Resolve issue with high CPU usage after moving Alacritty between displays
- Characters will no longer be deleted when using ncurses with the hard tab optimization
- Crash on non-linux operating systems when using the `SpawnNewInstance` action

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
- Fix the `Copy` `mouse_bindings` action ([#1963](https://github.com/jwilm/alacritty/issues/1963))
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
