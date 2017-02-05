Alacritty
=========

[![Build Status](https://travis-ci.org/jwilm/alacritty.svg?branch=master)](https://travis-ci.org/jwilm/alacritty)

Alacritty is the fastest terminal emulator in existence. Using the GPU for
rendering enables optimizations that simply aren't possible in other emulators.
Alacritty currently supports macOS and Linux, and Windows support is planned
before the 1.0 release.

<p align="center">
  <img width="600" alt="Alacritty running vim inside tmux" src="https://cloud.githubusercontent.com/assets/4285147/21585004/2ebd0288-d06c-11e6-95d3-4a2889dbbd6f.png">
</p>

## About

Alacritty is focused on simplicity and performance. The performance goal means
it should be faster than any other terminal emulator available. The simplicity
goal means that it doesn't have many features like tabs or scroll back as in
other terminals. Instead, it is expected that users of Alacritty make use of a
terminal multiplexer such as [`tmux`](https://github.com/tmux/tmux).

This initial release should be considered to be **pre-alpha** software--it will
have issues. Once Alacritty reaches an alpha level of readiness, precompiled
binaries will be provided for supported operating systems.

## Further information

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](http://blog.jwilm.io/announcing-alacritty/) January 6, 2017
- [A short talk about Alacritty at the Rust Meetup January 2017](https://air.mozilla.org/rust-meetup-january-2017/) (starts at 57:00)

## Package Installation

The only supported installation method at this time is from source. Proper installers will be added prior to the 1.0 release of Alacritty.

### Arch Linux

```sh
git clone https://aur.archlinux.org/alacritty-git.git
cd alacritty-git
makepkg -isr
```

## Manual Installation

### Prerequisites

1. Install [`rustup.rs`](https://rustup.rs/)

2. Clone the source code:

   ```sh
   git clone https://github.com/jwilm/alacritty.git
   cd alacritty
   ```

3. Make sure you have the right Rust compiler installed. Alacritty requires at least 1.15. Run

   ```sh
   rustup override set stable
   rustup update stable
   ```

#### Ubuntu

On Ubuntu, you need a few extra libraries to build Alacritty. Here's an `apt`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
apt-get install cmake libfreetype6-dev libfontconfig1-dev xclip
```

#### Arch Linux

On Arch Linux, you need a few extra libraries to build Alacritty. Here's a
`pacman` command that should install all of them. If something is still found
to be missing, please open an issue.

```sh
pacman -S cmake freetype2 fontconfig pkg-config make xclip
```

#### Fedora

On Fedora, you need a few extra libraries to build Alacritty. Here's a `dnf`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
dnf install cmake freetype-devel fontconfig-devel xclip
```

#### openSUSE

On openSUSE, you need a few extra libraries to build Alacritty. Here's
a `zypper` command that should install all of them. If something is
still found to be missing, please open an issue.

```sh
zypper install cmake freetype-devel fontconfig-devel xclip
```

#### Slackware

Compiles out of the box for 14.2
For copy & paste support (middle mouse button) you need to install xclip
https://slackbuilds.org/repository/14.2/misc/xclip/?search=xclip


#### Void Linux

On [Void Linux](https://voidlinux.eu), install following packages before compiling Alacritty:

```sh
xbps-install cmake freetype-devel freetype expat-devel fontconfig xclip
```

#### FreeBSD

On FreeBSD, you need a few extra libraries to build Alacritty. Here's a `pkg`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
pkg install cmake freetype2 fontconfig xclip
```

#### Other

If you build Alacritty on another distribution, we would love some help
filling in this section of the README.

### Building

Once all the prerequisites are installed, compiling Alacritty should be easy:

```sh
cargo build --release
```

If all goes well, this should place a binary at `target/release/alacritty`.
**BEFORE YOU RUN IT:** Install the config file as described below; otherwise,
many things (such as arrow keys) will not work. If you're on macOS, you'll need
to change the `monospace` font family to something like `Menlo`.

### Desktop Entry

Many linux distributions support desktop entries for adding applications to
system menus. To install the desktop entry for Alacritty, run

```sh
sudo cp target/release/alacritty /usr/local/bin # or anywhere else in $PATH
cp Alacritty.desktop ~/.local/share/applications
```

## Configuration

Although it's possible the default configuration would work on your system,
you'll probably end up wanting to customize it anyhow. There is a default
`alacritty.yml` at the git repository root. Alacritty looks for the configuration
file as the following paths:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
2. `$XDG_CONFIG_HOME/alacritty.yml`
3. `$HOME/.config/alacritty/alacritty.yml`
4. `$HOME/.alacritty.yml`

If neither of these paths are found then `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
is created once alacritty is first run. On most systems this often defaults
to `$HOME/.config/alacritty/alacritty.yml`.

Many configuration options will take effect immediately upon saving changes to
the config file. The only exception is the `font`, `dimensions` and `dpi` sections
which requires Alacritty to be restarted. For further explanation of the config
file, please consult the comments in the default config file.

## Issues (known, unknown, feature requests, etc)

If you run into a problem with Alacritty, please file an issue. If you've got a
feature request, feel free to ask about it. Keep in mind that Alacritty is very
much not looking to be a feature-rich terminal emulator with all sorts of bells
and widgets. It's primarily a cross-platform, blazing fast `tmux` renderer that
Just Works.

## FAQ

- _Is it really the fastest terminal emulator?_ In the terminals I've
  benchmarked against, alacritty is either faster, WAY faster, or at least
  neutral. There are no benchmarks in which I've found Alacritty to be slower.
- _It's not fast! Why?_ There's a known bug affecting some versions of
  Mesa/libxcb where calls to glClear take an insanely long time. If it's not
  that, there's probably another bug. I'd be happy to look at the issue if you
  can provide some profiling information (wall time and otherwise).
- _macOS + tmux + vim is slow! I thought this was supposed to be fast!_ This
  appears to be an issue outside of terminal emulators; either macOS has an IPC
  performance issue, or either tmux or vim (or both) have a bug. This same issue
  can be seen in `iTerm2` and `Terminal.app`. I've found that if tmux is running
  on another machine which is connected to Alacritty via SSH, this issue
  disappears. Actual throughput and rendering performance are still better in
  Alacritty.
- _Is wayland supported?_ Not yet. Alacritty is currently on a fork of glutin
  that needs some updates to work with Wayland. To stop glutin from detecting
  Wayland (e.g. for use on XWayland) launch Alacritty like this:
  `env WAYLAND_DISPLAY= alacritty`
- _When will Windows support be available?_ When someone has time to work on it.
  Contributors would be welcomed :).
- _My arrow keys don't work_. It sounds like you deleted some key bindings from
  your config file. Please reference the default config file to restore them.

## IRC

Alacritty discussion can be found in `#alacritty` on freenode.

## License

Alacritty is released under the [Apache License, Version 2.0].

[Apache License, Version 2.0]: https://github.com/jwilm/alacritty/blob/readme/LICENSE-APACHE
