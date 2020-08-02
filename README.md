<h1 align="center">Alacritty</h1>
<p align="center">
    <img width="200" alt="Alacritty Logo" src="extra/logo/compat/alacritty-term+scanlines.png">
</p>

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

Precompiled binaries are available from the [GitHub releases page](https://github.com/alacritty/alacritty/releases).

## Features

You can find an overview over the features available in Alacritty [here](./docs/features.md).

## Further information

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](https://jwilm.io/blog/announcing-alacritty/) January 6, 2017
- [A short talk about Alacritty at the Rust Meetup January 2017](https://air.mozilla.org/rust-meetup-january-2017/) (starts at 57:00)
- [Alacritty Lands Scrollback, Publishes Benchmarks](https://jwilm.io/blog/alacritty-lands-scrollback/) September 17, 2018
- [Version 0.3.0 Release Announcement](https://blog.christianduerr.com/alacritty_030_announcement) April 07, 2019
- [Version 0.5.0 Release Announcement](https://blog.christianduerr.com/alacritty_0_5_0_announcement) July 31, 2020

## Installation

Some operating systems already provide binaries for Alacritty, for everyone
else the instructions to build Alacritty from source can be found [here](INSTALL.md).

### Alpine Linux

```sh
apk add alacritty
```

### Arch Linux

```sh
pacman -S alacritty
```

### Fedora

Unofficial builds of stable tags can be found in Fedora Copr:
[pschyska/alacritty](https://copr.fedorainfracloud.org/coprs/pschyska/alacritty/).

``` sh
dnf copr enable pschyska/alacritty
dnf install alacritty
```

If you want to help test pre-releases, you can additionally enable
[pschyska/alacritty-testing](https://copr.fedorainfracloud.org/coprs/pschyska/alacritty-testing/).

### Gentoo Linux

```sh
emerge x11-terms/alacritty
```

### GNU Guix

```sh
guix package -i alacritty
```

### Mageia

```sh
urpmi alacritty
```

### NixOS

```sh
nix-env -iA nixos.alacritty
```

### openSUSE Tumbleweed

```sh
zypper in alacritty
```

### Pop!\_OS

```sh
apt install alacritty
```

### Solus

```sh
eopkg install alacritty
```

### Void Linux

```sh
xbps-install alacritty
```

### FreeBSD

```sh
pkg install alacritty
```

### macOS

```sh
brew cask install alacritty
```

Once the cask is installed, it is recommended to set up the manual page, shell
completions, and terminfo definitions. These are located inside the installed
application's Resources directory: `Alacritty.app/Contents/Resources`.

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

Prebuilt binaries for macOS, and Windows can be downloaded from the
[GitHub releases page](https://github.com/alacritty/alacritty/releases).

On Windows, Alacritty also requires Microsoft's VC++ redistributable.

For Windows versions older than Windows 10 (October 2018 Update), Alacritty
requires winpty to emulate UNIX's PTY API. The agent is a single binary
(`winpty-agent.exe`) which **must** be in the same directory as the Alacritty
executable and is available through the
[GitHub releases page](https://github.com/alacritty/alacritty/releases).

## Configuration

You can find the default configuration file with documentation for all available
fields on the [GitHub releases page](https://github.com/alacritty/alacritty/releases) for each release.

Alacritty doesn't create the config file for you, but it looks for one in the
following locations:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
2. `$XDG_CONFIG_HOME/alacritty.yml`
3. `$HOME/.config/alacritty/alacritty.yml`
4. `$HOME/.alacritty.yml`

### Windows

On Windows, the config file should be located at:

`%APPDATA%\alacritty\alacritty.yml`

## Contributing

A full guideline about contributing to Alacritty can be found in the
[`CONTRIBUTING.md`](CONTRIBUTING.md) file.

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

In the terminals we've [benchmarked](http://github.com/alacritty/vtebench),
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

## IRC

Alacritty discussion can be found in `#alacritty` on freenode.

## Wayland

Wayland is used by default on systems that support it. Using XWayland may
circumvent Wayland specific issues and can be enabled through:

```sh
env WINIT_UNIX_BACKEND=x11 alacritty
```

## License

Alacritty is released under the [Apache License, Version 2.0].

[Apache License, Version 2.0]: https://github.com/alacritty/alacritty/blob/master/LICENSE-APACHE
[tmux]: https://github.com/tmux/tmux
