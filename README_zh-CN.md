<p align="center">
    <img width="200" alt="Alacritty Logo" src="https://raw.githubusercontent.com/alacritty/alacritty/master/extra/logo/compat/alacritty-term%2Bscanlines.png">
</p>
<h1 align="center">Alacritty - 一个迅捷，跨平台，OpenGL驱动的终端模拟器</h1>

<p align="center">
  <img width="600"
       alt="Alacritty - A fast, cross-platform, OpenGL terminal emulator"
       src="https://user-images.githubusercontent.com/8886672/103264352-5ab0d500-49a2-11eb-8961-02f7da66c855.png">
</p>

## About

Alacritty 是一个现代化的、有着实用默认设置的终端模拟器，同时也支持海量的[自定义配置](#configuration)。通过与其他应用程序集成，而非重新实现其功能，它提供了一套灵活且具有很高性能的[特性](./docs/features_zh-CN.md)。
目前支持的系统平台有 BSD、Linux、macOS 和 Windows。

本软件目前仍处于**测试**阶段，仍存在少量缺失的特性并且有一些bug需要修复，但因它出色的表现，已经被许多人用于日常工作。

预编译的二进制文件可从 [GitHub Releases](https://github.com/alacritty/alacritty/releases) 页面获取。

## 特性

你可以在[这里](./docs/features_zh-CN.md)浏览 Alacritty 现在已具有的一系列特性。

## 更多信息

以下是关于 Alacritty 的一些文章、博客与视频。

- [Announcing Alacritty, a GPU-Accelerated Terminal Emulator](https://jwilm.io/blog/announcing-alacritty/) January 6, 2017
- [A talk about Alacritty at the Rust Meetup January 2017](https://www.youtube.com/watch?v=qHOdYO3WUTk) January 19, 2017
- [Alacritty Lands Scrollback, Publishes Benchmarks](https://jwilm.io/blog/alacritty-lands-scrollback/) September 17, 2018

## 安装

你可以通过多种包管理器在 Linux、BSD、macOS 和 Windows 平台上安装 Alacritty

适用于macOS和Windows平台的 Alacritty 预编译安装包可以在 [GitHub releases](https://github.com/alacritty/alacritty/releases) 页面下载。

对于其他平台的用户，安装 Alacritty 的详细说明可以在[这里](INSTALL_zh-CN.md)找到。

### 系统需求

- 要求至少 OpenGL ES 2.0 版本以上
- [Windows] 拥有 ConPTY 支持 (Windows 10，版本1809以上)

## 配置

对于每个 Releases 版本，你都可以在 [GitHub releases](https://github.com/alacritty/alacritty/releases) 页面找到默认配置文件 ([alacritty.yml](alacritty.yml))，默认配置文件中包含了所有可配置字段的文档。

Alacritty 不会为你创建一个默认的配置文件，但是它会在以下位置寻找你自定义的配置文件:

1. `$XDG_CONFIG_HOME/alacritty/alacritty.yml`
2. `$XDG_CONFIG_HOME/alacritty.yml`
3. `$HOME/.config/alacritty/alacritty.yml`
4. `$HOME/.alacritty.yml`

当在这些位置找不到配置文件时，它会使用自己的默认配置。

### Windows

在Windows平台上，配置文件应该放在这个位置:

`%APPDATA%\alacritty\alacritty.yml`

## 贡献

想为Alacritty作出贡献吗？请参照[`CONTRIBUTING.md`](CONTRIBUTING.md)文件.

## FAQ

**_这真的是最快的终端模拟器吗？_**

对终端模拟器进行 benchmark 非常复杂，Alacritty 使用
[vtebench](https://github.com/alacritty/vtebench) 来量化终端模拟器的吞吐量并且尝试去完善改进它以使它始终比使用它的竞争对手得分更高。如果您发现无法复现测试或测试结果并非期望那样，请提交一个 Bug。

延迟或帧速率和帧一致性等其他方面更难量化。某些终端模拟器还故意放慢速度以节省资源，某些用户可能更喜欢这种做法。

如果您对 Alacritty 的性能或可用性有疑问，衡量终端模拟器表现的最佳方法始终是使用**您**的特定用例对其进行测试。

**_为什么没有实现某特性？_**

Alacritty 有着非常多很棒的功能或特性, 但不可能兼具其他所有终端模拟器的所有特性。这可能是处于很多原因，但最关键的是，这不适合 Alacritty。 这意味着您不会找到诸如选项卡或拆分窗口之类的特性（最好留给窗口管理器或 [tmux](https://github.com/tmux/tmux)）或像 GUI 配置编辑器这样的细节。

## IRC

在 Libera.Chat 的 `#alacritty` 频道加入关于 Alacritty 的讨论！

## License

Alacritty 在 [Apache License, Version 2.0] 下发布.

[Apache License, Version 2.0]: https://github.com/alacritty/alacritty/blob/master/LICENSE-APACHE
