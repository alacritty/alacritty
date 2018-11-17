Alacritty
=========

[![Travis build Status](https://travis-ci.org/jwilm/alacritty.svg?branch=master)](https://travis-ci.org/jwilm/alacritty)
[![Appveyor build Status](https://ci.appveyor.com/api/projects/status/github/jwilm/alacritty?svg=true)](https://ci.appveyor.com/project/jwilm/alacritty)

Alacritty is the fastest terminal emulator in existence. Using the GPU for
rendering enables optimizations that simply aren't possible in other emulators.
Alacritty currently supports macOS, Linux, and Windows.

<p align="center">
  <img width="600" alt="Alacritty running vim inside tmux" src="https://cloud.githubusercontent.com/assets/4285147/21585004/2ebd0288-d06c-11e6-95d3-4a2889dbbd6f.png">
</p>

## About

Alacritty is focused on simplicity and performance. The performance goal means
it should be faster than any other terminal emulator available. The simplicity
goal means that it doesn't have features such as tabs or splits (which can be
better provided by a window manager or [terminal multiplexer][tmux]) nor
niceties like a GUI config editor.

The software is considered to be at an **alpha** level of readiness--there are
missing features and bugs to be fixed, but it is already used by many as a daily
driver.

Precompiled binaries are available from the [GitHub releases page](https://github.com/jwilm/alacritty/releases).

## Further information

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](https://jwilm.io/blog/announcing-alacritty/) January 6, 2017
- [A short talk about Alacritty at the Rust Meetup January 2017](https://air.mozilla.org/rust-meetup-january-2017/) (starts at 57:00)
- [Alacritty Lands Scrollback, Publishes Benchmarks](https://jwilm.io/blog/alacritty-lands-scrollback/) September 17, 2018

## Installation

Some operating systems already provide binaries for Alacritty, for everyone else the instructions
to build Alacritty from source can be found [here](INSTALL.md).

### Arch Linux

```sh
pacman -S alacritty
```

### openSUSE Tumbleweed

```sh
zypper in alacritty
```

### Void Linux

```sh
xbps-install alacritty
```

### Gentoo Linux

```sh
emerge x11-terms/alacritty
```

### FreeBSD

```sh
pkg install alacritty
```

### NixOS

```sh
nix-env -iA nixos.alacritty
```

### macOS

```sh
brew cask install alacritty
```

Once the cask is installed, it is recommended to setup the [manual page](INSTALL.md#manual-page), [shell completions](INSTALL.md#shell-completions), and [terminfo definitions](INSTALL.md#terminfo).

### Other

Prebuilt binaries for Linux, macOS, and Windows can be downloaded from the [GitHub releases page](https://github.com/jwilm/alacritty/releases).

## Configuration

Although it's possible the default configuration would work on your system,
you'll probably end up wanting to customize it anyhow. There is a default
`alacritty.yml`, `alacritty_macos.yml`, and `alacritty_windows.yml` at the Git repository root.
Alacritty looks for the configuration file as the following paths:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
2. `$XDG_CONFIG_HOME/alacritty.yml`
3. `$HOME/.config/alacritty/alacritty.yml`
4. `$HOME/.alacritty.yml`

If none of these paths are found then
`$XDG_CONFIG_HOME/alacritty/alacritty.yml` is created once Alacritty is first
run. On most systems this often defaults to
`$HOME/.config/alacritty/alacritty.yml`.

Many configuration options will take effect immediately upon saving changes to
the config file. The only exception is the `font` and `dimensions` sections
which requires Alacritty to be restarted. For further explanation of the config
file, please consult the comments in the default config file.

### Windows

On Windows the config file is located at:

`%UserProfile%\alacritty.yml`

## Issues (known, unknown, feature requests, etc.)

If you run into a problem with Alacritty, please file an issue. If you've got a
feature request, feel free to ask about it. Keep in mind that Alacritty is very
much not looking to be a feature-rich terminal emulator with all sorts of bells
and widgets. It's primarily a cross-platform, blazing fast `tmux` renderer that
Just Works.

## FAQ

**_Is it really the fastest terminal emulator?_**

In the terminals I've benchmarked against, Alacritty is either faster, WAY
faster, or at least neutral. There are no benchmarks in which I've found
Alacritty to be slower.

**_macOS + tmux + vim is slow! I thought this was supposed to be fast!_**

This appears to be an issue outside of terminal emulators; either macOS has an
IPC performance issue, or either tmux or vim (or both) have a bug. This same
issue can be seen in `iTerm2` and `Terminal.app`. I've found that if tmux is
running on another machine which is connected to Alacritty via SSH, this issue
disappears. Actual throughput and rendering performance are still better in
Alacritty.

**_My arrow keys don't work._**

It sounds like you deleted some key bindings from your config file. Please
reference the default config file to restore them.

## IRC

Alacritty discussion can be found in `#alacritty` on freenode.

## Wayland

Wayland support is available, but not everything works as expected. Many people
have found a better experience using XWayland which can be achieved launching
Alacritty with the `WAYLAND_DISPLAY` environment variable cleared:

```sh
env WAYLAND_DISPLAY= alacritty
```

If you're interested in seeing our Wayland support improve, please head over to
the [Wayland meta issue] on the _winit_ project to see how you may contribute.

## License

Alacritty is released under the [Apache License, Version 2.0].

[Apache License, Version 2.0]: https://github.com/jwilm/alacritty/blob/master/LICENSE-APACHE
[faq]: https://github.com/jwilm/alacritty#faq
[tmux]: https://github.com/tmux/tmux
[Wayland meta issue]: https://github.com/tomaka/winit/issues/306
