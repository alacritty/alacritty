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

## Blog Posts

There is a forthcoming announcement post the week of Jan 2, 2017.

## Installation

The only supported installation method at this time is from source. Proper
installers will be added prior to the 1.0 release of Alacritty. This section
will walk you through how to build from source on both macOS and Ubuntu.

### Prerequisites

1. Install [`rustup.rs`](https://rustup.rs/)

2. Clone the source code:

   ```sh
   git clone https://github.com/jwilm/alacritty
   cd alacritty
   ```

3. Make sure you have the right Rust compiler installed. Alacritty is currently
   pinned to a certain Rust nightly, and the compiler/nightly dependencies are
   updated as needed. To install the correct compiler, run:

   ```sh
   rustup override set $(cat rustc-version)
   ```

#### Additional Linux Prerequisites

##### Ubuntu

On Ubuntu, you need a few extra libraries to build Alacritty. Here's an `apt`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
apt-get install cmake libfreetype6-dev libfontconfig1-dev xclip
```

##### Arch Linux

On Arch Linux, you need a few extra libraries to build Alacritty. Here's a
`pacman` command that should install all of them. If something is still found
to be missing, please open an issue.
```sh
pacman -S cmake freetype2 fontconfig xclip
```

##### Fedora

On Fedora, you need a few extra libraries to build Alacritty. Here's a `dnf`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
dnf install freetype-devel fontconfig-devel xclip
```

##### Other

If you build Alacritty on another Linux distribution, we would love some help
filling in this section of the README.

### Building

Once all the prerequisites are installed, compiling Alacritty should be easy:

```sh
cargo build --release
```

If all goes well, this should place a binary at `target/release/alacritty`.
**BEFORE YOU RUN IT:** Install the config file as described below; otherwise,
many things (such as arrow keys) would not work. If you're on macOS, you'll need
to change the `monospace` font family to something like `Menlo`.

### Configuration

Although it's possible the default configuration would work on your system,
you'll probably end up wanting to customize it anyhow. There is an
`alacritty.yml` at the git repository root. Copy this to either
`$HOME/.alacritty.yml` or `$XDG_CONFIG_HOME/alacritty.yml` and run Alacritty.

Many configuration options will take effect immediately upon saving changes to
the config file. The only exception is the `font` and `dpi` section which
requires Alacritty to be restarted. For further explanation of the config file,
please consult the comments in the default config file.

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
  that needs some updates to work with Wayland.
- _When will Windows support be available?_ When someone has time to work on it.
  Contributors would be welcomed :).
- _My arrow keys don't work_ It sounds like you deleted some key bindings from
  your config file. Please reference the default config file to restore them.


## License

Alacritty is released under the [Apache License, Version 2.0].

[Apache License, Version 2.0]: https://github.com/jwilm/alacritty/blob/readme/LICENSE-APACHE
