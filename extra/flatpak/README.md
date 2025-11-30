# Flatpak Support for Alacritty

This directory contains the files needed to build Alacritty as a Flatpak.

## Prerequisites

Install the Flatpak SDK and runtime:

```sh
flatpak install flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08
flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable//25.08
```

Install `flatpak-builder`:

```sh
# Fedora
sudo dnf install flatpak-builder

# Debian/Ubuntu
sudo apt install flatpak-builder

# Arch Linux
sudo pacman -S flatpak-builder
```

## Building

Build and install locally:

```sh
make flatpak
```

Create a distributable bundle:

```sh
make flatpak-bundle
```

This creates `Alacritty.flatpak` which can be installed with:

```sh
flatpak install Alacritty.flatpak
```

## Running

```sh
flatpak run org.alacritty.Alacritty
```

## How It Works

The Flatpak uses [host-spawn](https://github.com/1player/host-spawn) to run
your shell on the host system with proper PTY allocation. A default config
at `/app/etc/xdg/alacritty/alacritty.toml` sets host-spawn as the shell program.

User configuration in `~/.config/alacritty/` takes precedence and is shared
with native installations.

## Permissions

The Flatpak has access to:
- Full host filesystem (`--filesystem=host`)
- GPU acceleration (`--device=dri`)
- Wayland and X11 display servers
- Network access
- Flatpak portal for host process spawning

Permissions can be adjusted with `flatpak override`.

## Files

- `org.alacritty.Alacritty.yml` - Flatpak manifest
- `org.alacritty.Alacritty.desktop` - Desktop entry
- `org.alacritty.Alacritty.metainfo.xml` - AppStream metadata
