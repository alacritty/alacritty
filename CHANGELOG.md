# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

### Changed

- Inverse/Selection color is now modelled after XTerm/VTE instead of URxvt to improve consistency
- First click on unfocused Alacritty windows is no longer ignored on platforms other than macOS

### Fixed

- Clear screen properly before rendering of content to prevent various graphical glitches
- Fix build failure on 32-bit systems
- Windows started as unfocused now show the hollow cursor if the setting is enabled

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
