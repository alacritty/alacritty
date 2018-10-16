# Manual Installation

1. [Prerequisites](#prerequisites)
    1. [Source Code](#clone-the-source-code)
    2. [Rust Compiler](#install-the-rust-compiler-with-rustup)
    3. [Dependencies](#dependencies)
        1. [Debian/Ubuntu](#debianubuntu)
        2. [Arch Linux](#arch-linux)
        3. [Fedora](#fedora)
        4. [CentOS/RHEL 7](#centosrhel-7)
        5. [openSUSE](#opensuse)
        6. [Slackware](#slackware)
        7. [Void Linux](#void-linux)
        8. [FreeBSD](#freebsd)
        9. [OpenBSD](#openbsd)
        10. [Solus](#solus)
        11. [NixOS/Nixpkgs](#nixosnixpkgs)
        12. [Gentoo](#gentoo)
        13. [Other](#other)
2. [Building](#building)
    1. [Linux](#linux)
        1. [Desktop Entry](#desktop-entry)
    2. [MacOS](#macos)
    3. [Cargo](#cargo)
3. [Manual Page](#manual-page)
4. [Shell Completions](#shell-completions)
    1. [Zsh](#zsh)
    2. [Bash](#bash)
    3. [Fish](#fish)
5. [Terminfo](#terminfo)

## Prerequisites

### Clone the source code

Before compiling Alacritty, you'll have to first clone the source code:

```sh
git clone https://github.com/jwilm/alacritty.git
cd alacritty
```

### Install the Rust compiler with `rustup`

1. Install [`rustup.rs`](https://rustup.rs/).

3. To make sure you have the right Rust compiler installed, run

   ```sh
   rustup override set stable
   rustup update stable
   ```

### Dependencies

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

Note that you still need to download system build dependencies via your package
manager as mentioned above. The binary `alacritty` will be placed into `$HOME/.cargo/bin`.
Make sure it is in your path (default if you use `rustup`).

#### Other

If you build Alacritty on another distribution, we would love some help
filling in this section of the README.

## Building

### Linux

Once all the prerequisites are installed, compiling Alacritty should be easy:

```sh
cargo build --release
```

If all goes well, this should place a binary at `target/release/alacritty`.

#### Desktop Entry

Many linux distributions support desktop entries for adding applications to
system menus. To install the desktop entry for Alacritty, run

```sh
sudo cp target/release/alacritty /usr/local/bin # or anywhere else in $PATH
sudo desktop-file-install alacritty.desktop
sudo update-desktop-database
```

### MacOS

To build an application for macOS, run

```sh
make app
cp -r target/release/osx/Alacritty.app /Applications/
```

### Cargo

If you don't want to clone the repository, you can install Alacritty directly using cargo:

```sh
cargo install --git https://github.com/jwilm/alacritty
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

## Terminfo

The terminfo database contains entries describing the terminal
emulator's capabilities. Programs need these in order to function
properly.

Alacritty should work with the standard `xterm-256color` definition,
but to allow programs to make best use of alacritty's capabilities,
use its own terminfo definition instead.

Unless the user has set the `TERM` environment variable in the
alacritty configuration, the `alacritty` terminfo definition will be
used if it has been installed. If not, then `xterm-256color` is used
instead.

To install alacritty's terminfo entry globally:

```sh
sudo tic -e alacritty,alacritty-direct alacritty.info
```
