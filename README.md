Alacritty
=========

[![Travis build Status](https://travis-ci.org/jwilm/alacritty.svg?branch=master)](https://travis-ci.org/jwilm/alacritty)
[![Appveyor build Status](https://ci.appveyor.com/api/projects/status/sjq7ya5xf3hsa59s?svg=true)](https://ci.appveyor.com/project/zacps/alacritty)

Alacritty is the fastest terminal emulator in existence. Using the GPU for
rendering enables optimizations that simply aren't possible in other emulators.
Alacritty currently supports macOS and Linux, and Windows.

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

Precompiled binaries are available for Windows through [appveyor](https://ci.appveyor.com/project/jwilm/alacritty).
Precompiled binaries for other platforms are minimally blocked on a stable config format. For now, Alacritty must be
built from source.

## Further information

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](http://jwilm.io/blog/announcing-alacritty/) January 6, 2017
- [A short talk about Alacritty at the Rust Meetup January 2017](https://air.mozilla.org/rust-meetup-january-2017/) (starts at 57:00)
- [Alacritty Lands Scrollback, Publishes Benchmarks](https://jwilm.io/blog/alacritty-lands-scrollback/) September 17, 2018

## Installation

Some operating systems already provide binaries for Alacritty, for everyone else there are
instructions to compile Alacritty from source.

For the manual installation, please first read the [prerequisites](#prerequisites) section,
then find the instructions for your OS, and finally go through the [building](#building)
and [configuration](#configuration) steps.

### Arch Linux

```sh
pacman -S alacritty
```

### Debian/Ubuntu

Using `cargo deb`, you can create and install a deb file.

```sh
git clone https://github.com/jwilm/alacritty.git
cd alacritty
cargo install cargo-deb
cargo deb --install
```

### openSUSE Tumbleweed Linux

```sh
zypper in alacritty
```

### Void Linux

```sh
xbps-install alacritty
```

## Manual Installation

### Prerequisites

1. Alacritty requires the most recent stable Rust compiler; it can be installed with
   `rustup`.

#### Installing Rust compiler with `rustup`

1. Install [`rustup.rs`](https://rustup.rs/).

2. Clone the source code:

   ```sh
   git clone https://github.com/jwilm/alacritty.git
   cd alacritty
   ```

3. Make sure you have the right Rust compiler installed. Run

   ```sh
   rustup override set stable
   rustup update stable
   ```

#### Debian/Ubuntu

You can build alacritty using `cargo deb` and use your system's package manager
to maintain the application using the instructions [above](#debianubuntu).

If you'd still like to build a local version manually, you need a few extra
libraries to build Alacritty. Here's an apt command that should install all of
them. If something is still found to be missing, please open an issue.

```sh
apt-get install cmake libfreetype6-dev libfontconfig1-dev xclip
```

#### Windows

On windows you will need to have the `{architecture}-pc-windows-msvc` toolchain installed as well as [Clang 3.9 or greater](http://releases.llvm.org/download.html).

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

#### CentOS/RHEL 7

On CentOS/RHEL 7, you need a few extra libraries to build Alacritty. Here's a `yum`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
yum install cmake freetype-devel fontconfig-devel xclip
yum group install "Development Tools"
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

On [Void Linux](https://voidlinux.eu), install following packages before
compiling Alacritty:

```sh
xbps-install cmake freetype-devel freetype expat-devel fontconfig-devel fontconfig xclip
```

#### FreeBSD

On FreeBSD, you need a few extra libraries to build Alacritty. Here's a `pkg`
command that should install all of them. If something is still found to be
missing, please open an issue.

```sh
pkg install cmake freetype2 fontconfig xclip pkgconf
```

#### OpenBSD

Alacritty builds on OpenBSD 6.3 almost out-of-the-box if Rust and
[Xenocara](https://xenocara.org) are installed.  If something is still found to
be missing, please open an issue.

```sh
pkg_add rust
```

#### Solus

On [Solus](https://solus-project.com/), you need a few extra libraries to build
Alacritty. Here's a `eopkg` command that should install all of them. If
something is still found to be missing, please open an issue.

```sh
sudo eopkg install fontconfig-devel
```

#### NixOS/Nixpkgs

The following command can be used to get a shell with all development
dependencies on [NixOS](https://nixos.org).

```sh
nix-shell -A alacritty '<nixpkgs>'
```

#### Gentoo

On Gentoo, there's a portage overlay available. Make sure `layman` is installed
and run:

```sh
sudo layman -a slyfox
```

Then, add `x11-terms/alacritty **` to `/etc/portage/package.accept_keywords` and
emerge alacritty:

```sh
sudo emerge alacritty
```

It might be handy to mask all other packages provided in the `slyfox` overlay by
adding `*/*::slyfox` to `/etc/portage/package.mask` and adding
`x11-terms/alacritty::slyfox` to `/etc/portage/package.unmask`.

### Cargo

If you have a rust toolchain setup you can install Alacritty via cargo:

```sh
cargo install --git https://github.com/jwilm/alacritty
```

Note that you still need to download system build dependencies via your package	
manager as mentioned above. The binary `alacritty` will be placed into `$HOME/.cargo/bin`.	
Make sure it is in your path (default if you use `rustup`).	

#### Other

If you build Alacritty on another distribution, we would love some help
filling in this section of the README.

### Building

**BEFORE YOU RUN IT:** Install the config file as described below; otherwise,
many things (such as arrow keys) will not work.

#### Linux

Once all the prerequisites are installed, compiling Alacritty should be easy:

```sh
cargo build --release
```

If all goes well, this should place a binary at `target/release/alacritty`.


##### Desktop Entry

Many linux distributions support desktop entries for adding applications to
system menus. To install the desktop entry for Alacritty, run

```sh
sudo cp target/release/alacritty /usr/local/bin # or anywhere else in $PATH
sudo desktop-file-install alacritty.desktop
sudo update-desktop-database
```

#### MacOS

To build an application for macOS, run

```sh
make app
cp -r target/release/osx/Alacritty.app /Applications/
```

## Manual Page

Installing the manual page requires the additional dependency `gzip`.
To install the manual page, run

```sh
sudo mkdir -p /usr/local/share/man/man1
gzip -c alacritty.man | sudo tee /usr/local/share/man/man1/alacritty.1.gz > /dev/null
```

## Shell completions

To get automatic completions for alacritty's flags and arguments you can install the provided shell completions.

### Zsh

To install the completions for zsh, you can place the `alacritty-completions.zsh` as `_alacritty` in any directory referenced by `$fpath`.

If you do not already have such a directory registered through your `~/.zshrc`, you can add one like this:

```sh
mkdir -p ${ZDOTDIR:-~}/.zsh_functions
echo 'fpath+=${ZDOTDIR:-~}/.zsh_functions' >> ${ZDOTDIR:-~}/.zshrc
```

Then copy the completion file to this directory:

```sh
cp alacritty-completions.zsh ${ZDOTDIR:-~}/.zsh_functions/_alacritty
```

### Bash

To install the completions for bash, you can `source` the `alacritty-completions.bash` in your `~/.bashrc` file.

If you do not plan to delete the source folder of alacritty, you can run

```sh
echo "source $(pwd)/alacritty-completions.bash" >> ~/.bashrc
```

Otherwise you can copy it to the `~/.bash_completion` folder and source it from there:

```sh
mkdir -p ~/.bash_completion
cp alacritty-completions.bash ~/.bash_completion/alacritty
echo "source ~/.bash_completion/alacritty" >> ~/.bashrc
```

### Fish

To install the completions for fish, run

```
sudo cp alacritty-completions.fish $__fish_datadir/vendor_completions.d/alacritty.fish
```

## Configuration

Although it's possible the default configuration would work on your system,
you'll probably end up wanting to customize it anyhow. There is a default
`alacritty.yml`, `alacritty_macos.yml`, and `alacritty_windows.yml` at the git repository root.
Alacritty looks for the configuration file as the following paths:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
2. `$XDG_CONFIG_HOME/alacritty.yml`
3. `$HOME/.config/alacritty/alacritty.yml`
4. `$HOME/.alacritty.yml`

If none of these paths are found then
`$XDG_CONFIG_HOME/alacritty/alacritty.yml` is created once alacritty is first
run. On most systems this often defaults to
`$HOME/.config/alacritty/alacritty.yml`.

Many configuration options will take effect immediately upon saving changes to
the config file. The only exception is the `font` and `dimensions` sections
which requires Alacritty to be restarted. For further explanation of the config
file, please consult the comments in the default config file.

### Windows

On windows the config file is located at:

`%UserProfile%\alacritty.yml`

## Issues (known, unknown, feature requests, etc)

If you run into a problem with Alacritty, please file an issue. If you've got a
feature request, feel free to ask about it. Keep in mind that Alacritty is very
much not looking to be a feature-rich terminal emulator with all sorts of bells
and widgets. It's primarily a cross-platform, blazing fast `tmux` renderer that
Just Works.

## FAQ

**_Is it really the fastest terminal emulator?_**

In the terminals I've benchmarked against, alacritty is either faster, WAY
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
