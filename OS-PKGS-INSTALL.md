# OS Package Installation

- [Alpine Linux](#alpine-linux)
- [Arch Linux](#arch-linux)
- [Fedora](#fedora)
- [Gentoo Linux](#gentoo-linux)
- [GNU Guix](#gnu-guix)
- [Mageia](#mageia)
- [NixOS](#nixos)
- [openSUSE Tumbleweed](#opensuse-tumbleweed)
- [Pop!\_OS](#pop_os)
- [Solus](#solus)
- [Void Linux](#void-linux)
- [FreeBSD](#freebsd)
- [macOS](#macos)
- [Windows](#windows)
  - [Via Chocolatey](#via-chocolatey)
  - [Via Scoop](#via-scoop)

## Alpine Linux

```sh
apk add alacritty
```

## Arch Linux

```sh
pacman -S alacritty
```

## Fedora

Unofficial builds of stable tags can be found in Fedora Copr:
[pschyska/alacritty](https://copr.fedorainfracloud.org/coprs/pschyska/alacritty/).

``` sh
dnf copr enable pschyska/alacritty
dnf install alacritty
```

If you want to help test pre-releases, you can additionally enable
[pschyska/alacritty-testing](https://copr.fedorainfracloud.org/coprs/pschyska/alacritty-testing/).

## Gentoo Linux

```sh
emerge x11-terms/alacritty
```

## GNU Guix

```sh
guix package -i alacritty
```

## Mageia

```sh
urpmi alacritty
```

## NixOS

```sh
nix-env -iA nixos.alacritty
```

## openSUSE Tumbleweed

```sh
zypper in alacritty
```

## Pop!\_OS

```sh
apt install alacritty
```

## Solus

```sh
eopkg install alacritty
```

## Void Linux

```sh
xbps-install alacritty
```

## FreeBSD

```sh
pkg install alacritty
```

## macOS

```sh
brew cask install alacritty
```

Once the cask is installed, it is recommended to setup the manual page, shell
completions, and terminfo definitions. These are located inside the installed
application's Resources directory: `Alacritty.app/Contents/Resources`.

## Windows

### Via [Chocolatey](https://chocolatey.org)

```batch
choco install alacritty
```

### Via [Scoop](https://scoop.sh)

```batch
scoop bucket add extras
scoop install alacritty
```