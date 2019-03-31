<p align="center">
    <img width="250" alt="Alacritty Logo" src="extra/logo/alacritty.svg">
</p>

Alacritty
=========

[![Travis build Status](https://travis-ci.org/jwilm/alacritty.svg?branch=master)](https://travis-ci.org/jwilm/alacritty)

Alacritty is the fastest terminal emulator in existence. Using the GPU for
rendering enables optimizations that simply aren't possible without it.
Alacritty currently supports macOS, Linux, BSD, and Windows.

<p align="center">
  <img width="600"
       alt="Alacritty running vim inside tmux"
       src="https://cloud.githubusercontent.com/assets/4285147/21585004/2ebd0288-d06c-11e6-95d3-4a2889dbbd6f.png">
</p>

## About

Alacritty is a terminal emulator with a strong focus on simplicity and
performance. With such a strong focus on performance, included features are
carefully considered and you can always expect Alacritty to be blazingly fast.
By making sane choices for defaults, Alacritty requires no additional setup.
However, it does allow [configuration](#configuration) of many aspects of the
terminal.

The software is considered to be at a **beta** level of readiness -- there are
a few missing features and bugs to be fixed, but it is already used by many as
a daily driver.

Precompiled binaries are available from the [GitHub releases page](https://github.com/jwilm/alacritty/releases).

## Further information

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](https://jwilm.io/blog/announcing-alacritty/) January 6, 2017
- [A short talk about Alacritty at the Rust Meetup January 2017](https://air.mozilla.org/rust-meetup-january-2017/) (starts at 57:00)
- [Alacritty Lands Scrollback, Publishes Benchmarks](https://jwilm.io/blog/alacritty-lands-scrollback/) September 17, 2018

## Installation

Some operating systems already provide binaries for Alacritty, for everyone
else the instructions to build Alacritty from source can be found [here](INSTALL.md).

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

### Mageia 7+

```sh
urpmi alacritty
```

### FreeBSD

```sh
pkg install alacritty
```

### NixOS

```sh
nix-env -iA nixos.alacritty
```

### Solus

```sh
eopkg install alacritty
```

### macOS

```sh
brew cask install alacritty
```

Once the cask is installed, it is recommended to setup the [manual page](INSTALL.md#manual-page),
[shell completions](INSTALL.md#shell-completions), and [terminfo definitions](INSTALL.md#terminfo).

### Windows

#### Via [Chocolatey](https://chocolatey.org)

```batch
choco install alacritty
```

#### Via [Scoop](https://scoop.sh)

```batch
scoop bucket add extras
scoop install alacritty
```

### Other

Prebuilt binaries for Linux, macOS, and Windows can be downloaded from the
[GitHub releases page](https://github.com/jwilm/alacritty/releases).

To work properly on Windows, Alacritty requires winpty to emulate UNIX's PTY
API. The agent is a single binary (`winpty-agent.exe`) which **must** be in
the same directory as the Alacritty executable and is available through the
[GitHub releases page](https://github.com/jwilm/alacritty/releases).

## Configuration

Although it's possible the default configuration would work on your system,
you'll probably end up wanting to customize it anyhow. There is a default
`alacritty.yml` at the Git repository root. Alacritty looks for the
configuration file at the following paths:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
2. `$XDG_CONFIG_HOME/alacritty.yml`
3. `$HOME/.config/alacritty/alacritty.yml`
4. `$HOME/.alacritty.yml`

If none of these paths are found then
`$XDG_CONFIG_HOME/alacritty/alacritty.yml` is created once Alacritty is first
run. On most systems this often defaults to
`$HOME/.config/alacritty/alacritty.yml`.

Many configuration options will take effect immediately upon saving changes to
the config file. For more information about the config file structure, refer to
the default config file.

### Windows

On Windows the config file is located at:

`%APPDATA%\alacritty\alacritty.yml`

## Issues (known, unknown, feature requests, etc.)

If you run into a problem with Alacritty, please file an issue. If you've got a
feature request, feel free to ask about it. Please just keep in mind Alacritty
is focused on simplicity and performance, and not all features are in line with
that goal.

Before opening a new issue, please check if it has already been reported.
There's a chance someone else has already reported it, and you can subscribe to
that issue to keep up on the latest developments.

## FAQ

**_Is it really the fastest terminal emulator?_**

In the terminals we've [benchmarked](http://github.com/jwilm/vtebench),
Alacritty is either faster or **way** faster than the others. If you've found a
case where this isn't true, please report a bug.

**_Why isn't feature X implemented?_**

Alacritty has many great features, but not every feature from every other
terminal. This could be for a number of reasons, but sometimes it's just not a
good fit for Alacritty. This means you won't find things like tabs or splits
(which are best left to a window manager or [terminal multiplexer][tmux]) nor
niceties like a GUI config editor.

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
have found a better experience using XWayland which can be achieved by
launching Alacritty with the `WAYLAND_DISPLAY` environment variable cleared:

```sh
env WAYLAND_DISPLAY="" alacritty
```

If you're interested in seeing our Wayland support improve, please head over to
the [Wayland meta issue] on the _winit_ project to see how you may contribute.

## License

Alacritty is released under the [Apache License, Version 2.0].

[Apache License, Version 2.0]: https://github.com/jwilm/alacritty/blob/master/LICENSE-APACHE
[faq]: https://github.com/jwilm/alacritty#faq
[tmux]: https://github.com/tmux/tmux
[Wayland meta issue]: https://github.com/tomaka/winit/issues/306
