# Changelog

All notable changes to alacritty_terminal are documented in this file. The
sections should follow the order `Added`, `Changed`, `Deprecated`, `Fixed` and
`Removed`.

**Breaking changes are written in bold style.**

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## 0.24.1-dev

## 0.24.0

### Added

- `tty::unix::from_fd()` to create a TTY from a pre-opened PTY's file-descriptors

### Changed

- **`Term` is not focused by default anymore**
