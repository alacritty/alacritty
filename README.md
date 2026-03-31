<p align="center">
    <img width="200" alt="Alacritty Logo" src="https://raw.githubusercontent.com/alacritty/alacritty/master/extra/logo/compat/alacritty-term%2Bscanlines.png">
</p>

<h1 align="center">Alacritty - A fast, cross-platform, OpenGL terminal emulator</h1>

## This Fork

This repository is a fork of Alacritty that adds in-window tabs, including a
configurable tab bar, tab actions, and custom tab titles, similar to Kitty's
tab workflow.

Upstream Alacritty has historically declined adding tabs by design; see
[Tabs support in the terminal (#3129)](https://github.com/alacritty/alacritty/issues/3129),
which is closed with the `F - wontfix` label.

<p align="center">
<img width="1000" alt="alacritty tabs" src="https://github.com/user-attachments/assets/1abb3b24-19a7-417d-a277-98652552d3cf" />
</p>

## Example Tabs Config

```toml
[tabs]
tab_bar_edge = "top"
tab_bar_style = "slant"
tab_powerline_style = "slanted"
tab_bar_min_tabs = 1
tab_switch_strategy = "previous"
tab_title_template = "{title}"
active_tab_foreground = "#1e1e2e"
active_tab_background = "#cba6f7"
active_tab_font_style = "italic"
inactive_tab_foreground = "#cdd6f4"
inactive_tab_background = "#0b0b12"
inactive_tab_font_style = "normal"
tab_bar_background = "#11111b"
mouse = { enabled = true, hover = true }

[keyboard]
bindings = [
  { key = "T", mods = "Super", action = "CreateNewTab" },
  { key = "Right", mods = "Super", action = "SelectNextTab" },
  { key = "Left", mods = "Super", action = "SelectPreviousTab" },
  { key = "Tab", mods = "Super", action = "SelectNextTab" },
  { key = "Tab", mods = "Super|Shift", action = "SelectPreviousTab" },
  { key = "W", mods = "Super", action = "CloseTab" },
  { key = ".", mods = "Super", action = "MoveTabForward" },
  { key = ",", mods = "Super", action = "MoveTabBackward" },
  { key = "T", mods = "Super|Alt", action = "SetTabTitle" },
]
```

## About

Alacritty is a modern terminal emulator that comes with sensible defaults, but
allows for extensive [configuration](#configuration). By integrating with other
applications, rather than reimplementing their functionality, it manages to
provide a flexible set of [features](./docs/features.md) with high performance.
The supported platforms currently consist of BSD, Linux, macOS and Windows.

The software is considered to be at a **beta** level of readiness; there are
a few missing features and bugs to be fixed, but it is already used by many as
a daily driver.

Precompiled binaries are available from the [GitHub releases page](https://github.com/alacritty/alacritty/releases).

Join [`#alacritty`] on libera.chat if you have questions or looking for a quick help.

[`#alacritty`]: https://web.libera.chat/gamja/?channels=#alacritty

## Features

You can find an overview over the features available in Alacritty [here](./docs/features.md).

## Further information

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](https://jwilm.io/blog/announcing-alacritty/) January 6, 2017
- [A talk about Alacritty at the Rust Meetup January 2017](https://www.youtube.com/watch?v=qHOdYO3WUTk) January 19, 2017
- [Alacritty Lands Scrollback, Publishes Benchmarks](https://jwilm.io/blog/alacritty-lands-scrollback/) September 17, 2018

## Installation

Alacritty can be installed by using various package managers on Linux, BSD,
macOS and Windows.

Prebuilt binaries for macOS and Windows can also be downloaded from the
[GitHub releases page](https://github.com/alacritty/alacritty/releases).

For everyone else, the detailed instructions to install Alacritty can be found
[here](INSTALL.md).

### Requirements

- At least OpenGL ES 2.0
- [Windows] ConPTY support (Windows 10 version 1809 or higher)

## Configuration

You can find the documentation for Alacritty's configuration in `man 5
alacritty`, or by looking at [the website] if you do not have the manpages
installed.

[the website]: https://alacritty.org/config-alacritty.html

Alacritty doesn't create the config file for you, but it looks for one in the
following locations:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.toml`
2. `$XDG_CONFIG_HOME/alacritty.toml`
3. `$HOME/.config/alacritty/alacritty.toml`
4. `$HOME/.alacritty.toml`
5. `/etc/alacritty/alacritty.toml`

On Windows, the config file will be looked for in:

* `%APPDATA%\alacritty\alacritty.toml`

## Contributing

A guideline about contributing to Alacritty can be found in the
[`CONTRIBUTING.md`](CONTRIBUTING.md) file.

## FAQ

**_Is it really the fastest terminal emulator?_**

Benchmarking terminal emulators is complicated. Alacritty uses
[vtebench](https://github.com/alacritty/vtebench) to quantify terminal emulator
throughput and manages to consistently score better than the competition using
it. If you have found an example where this is not the case, please report a
bug.

Other aspects like latency or framerate and frame consistency are more difficult
to quantify. Some terminal emulators also intentionally slow down to save
resources, which might be preferred by some users.

If you have doubts about Alacritty's performance or usability, the best way to
quantify terminal emulators is always to test them with **your** specific
usecases.

**_Why isn't feature X implemented?_**

Alacritty has many great features, but not every feature from every other
terminal. This could be for a number of reasons, but sometimes it's just not a
good fit for Alacritty. This means you won't find things like splits (which are
best left to a window manager or [terminal multiplexer][tmux]) nor niceties
like a GUI config editor.

[tmux]: https://github.com/tmux/tmux

## License

Alacritty is released under the [Apache License, Version 2.0].

[Apache License, Version 2.0]: https://github.com/alacritty/alacritty/blob/master/LICENSE-APACHE
