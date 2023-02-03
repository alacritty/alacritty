# 使用Cargo安装

如果您只对使用 Alacritty 感兴趣，且不需要
[终端信息文件](#terminfo)， [添加桌面快捷方式](#添加桌面快捷方式)，[手册(man)](#手册(man)) 或 [Shell 补全](#Shell-补全)， 您可以直接通过Cargo安装：

```sh
cargo install alacritty
```

注意，您仍然需要基于您的平台安装依赖文件，详情请参照 [依赖](#依赖)。

# 手动安装

1. [先决条件](#先决条件)
   1. [克隆源代码](#克隆源代码)
   2. [使用 `rustup` 安装 Rust 编译器](#使用-rustup-安装-rust-编译器)
   3. [依赖](#依赖)
      1. [Debian/Ubuntu](#debianubuntu)
      2. [Arch Linux](#arch-linux)
      3. [Fedora](#fedora)
      4. [CentOS/RHEL 7](#centosrhel-7)
      5. [RHEL 8](#rhel-8)
      6. [openSUSE](#opensuse)
      7. [Slackware](#slackware)
      8. [Void Linux](#void-linux)
      9. [FreeBSD](#freebsd)
      10. [OpenBSD](#openbsd)
      11. [Solus](#solus)
      12. [NixOS/Nixpkgs](#nixosnixpkgs)
      13. [Gentoo](#gentoo)
      14. [Clear Linux](#clear-linux)
      15. [GNU Guix](#gnu-guix)
      16. [Alpine Linux](#alpine-linux)
      17. [Windows](#windows)
      18. [其他平台](#其他平台)
2. [构建/生成](#构建生成)
   1. [Linux / Windows / BSD](#linux--windows--bsd)
   2. [macOS](#macos)
      1. [同时支持 x86 和 ARM 架构](#同时支持-x86-和-arm-架构)
3. [构建之后](#构建之后)
   1. [Terminfo](#terminfo)
   2. [添加桌面快捷方式](#添加桌面快捷方式)
   3. [手册(man)](#手册man)
   4. [Shell 补全](#shell-补全)
      1. [Zsh](#zsh)
      2. [Bash](#bash)
      3. [Fish](#fish)



## 先决条件

### 克隆源代码

在编译 Alacritty 之前，您需要先克隆源代码:

```sh
git clone https://github.com/alacritty/alacritty.git
cd alacritty
```

### 使用 `rustup` 安装 Rust 编译器

1. 安装 [`rustup.rs`](https://rustup.rs/).

3. 为了确保您正确安装了 Rust 编译器，请运行

   ```sh
   rustup override set stable
   rustup update stable
   ```

### 依赖

这些是生成 Alacritty 所需的最小依赖, 请注意，对于某些设置，可能需要额外的依赖项。

如果您使用 Nvidia GPU 运行 Wayland ，则可能也需要为 EGL 安装驱动程序(这些在 Ubuntu 上称为    `libegl1-mesa-dev`)。

#### Debian/Ubuntu

如果您想手动构建本地版本，您需要一些额外的库来构建  Alacritty 。这是一个应该安装所有需要库的 `apt` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
apt-get install cmake pkg-config libfreetype6-dev libfontconfig1-dev libxcb-xfixes0-dev libxkbcommon-dev python3
```

#### Arch Linux

在 Arch Linux 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `pacman` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
pacman -S cmake freetype2 fontconfig pkg-config make libxcb libxkbcommon python
```

#### Fedora

在 Fedora 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `dnf` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel g++
```

#### CentOS/RHEL 7

在 CentOS/RHEL 7 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `yum` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
yum install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel xcb-util-devel
yum group install "Development Tools"
```

#### RHEL 8

在 RHEL 8 上，与 RHEL 7 上类似，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `dnf` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
dnf install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel
dnf group install "Development Tools"
```

#### openSUSE

在 openSUSE 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `zypper` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
zypper install cmake freetype-devel fontconfig-devel libxcb-devel libxkbcommon-devel
```

#### Slackware

开箱即用的 14.2 编译版本。

#### Void Linux

在 [Void Linux](https://voidlinux.org) 上, 在编译 Alacritty 之前请先安装以下包。

```sh
xbps-install cmake freetype-devel expat-devel fontconfig-devel libxcb-devel pkg-config python3
```

#### FreeBSD

在 FreeBSD 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `pkg` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
pkg install cmake freetype2 fontconfig pkgconf python3
```

#### OpenBSD

在 OpenBSD 6.5 上, 您需要用到 [Xenocara](https://xenocara.org) 和 Rust 构建 Alacritty，以及 Python 3 来构建它的 XCB 依赖。如果仍然有某些内容缺失，请创建一个 issue。

```sh
pkg_add rust python
```

出现提示时，选择 Python 3 的包（例如 `python-3.6.8p0`）。

OpenBSD 中的默认用户限制不足以构建 Alacritty。建议至少 3GB 的数据大小(请参阅 [login.conf](https://man.openbsd.org/login.conf)))。

#### Solus

在 [Solus](https://solus-project.com/) 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `eopkg` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
eopkg install fontconfig-devel
```

#### NixOS/Nixpkgs

以下命令可用于获取 [NixOS](https://nixos.org) 上所有开发依赖项的 shell。

```sh
nix-shell -A alacritty '<nixpkgs>'
```

#### Gentoo

在 Gentoo 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `emerge` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
emerge --onlydeps x11-terms/alacritty
```

#### Clear Linux

在 Clear Linux 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `swupd` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
swupd bundle-add devpkg-expat devpkg-freetype devpkg-libxcb devpkg-fontconfig
```

#### GNU Guix

以下命令可用于获取 [GNU Guix](https://guix.gnu.org/) 上所有开发依赖项的 shell。

```sh
guix environment alacritty
```

#### Alpine Linux

在 Alpine Linux 上，您需要一些额外的库来构建 Alacritty。这是一个应该安装所有需要库的 `apk` 命令。如果仍然有某些内容缺失，请创建一个 issue。

```sh
sudo apk add cmake pkgconf freetype-dev fontconfig-dev python3 libxcb-dev
```

#### Windows

在 Windows 上您需要安装 `{architecture}-pc-windows-msvc` 工具链以及 [Clang 3.9 或以上版本](http://releases.llvm.org/download.html).

#### 其他平台

如果您在另一个发行版上准备构建 Alacritty，我们会很希望有人帮助填写自述文件的这一部分。

## 构建/生成

在构建之后，会生成 Alacritty 的可执行二进制文件。

### Linux / Windows / BSD

```sh
cargo build --release
```

在 Linux/BSD 上，如果不需要构建对 X11 与 Wayland 渲染同时支持的 Alacritty，可以使用以下命令。

```sh
# 强制只支持Wayland
cargo build --release --no-default-features --features=wayland

# 强制只支持X11
cargo build --release --no-default-features --features=x11
```

如果一切运行成功，那么会在 `target/release/alacritty` 生成 Alacritty 的可运行二进制文件。

### macOS

```sh
make app
cp -r target/release/osx/Alacritty.app /Applications/
```

#### 同时支持 x86 和 ARM 架构

以下内容将构建在 x86 和 ARM macos 架构上皆可运行的可执行文件：

```sh
rustup target add x86_64-apple-darwin aarch64-apple-darwin
make app-universal
```

## 构建之后

安装 Alacritty 后，您可能想要设置一些额外的内容。
所有的构建后命令都假设您还在 Alacritty 仓库中。

### Terminfo

为了确保 Alacritty 运行正常, `alacritty` 和
`alacritty-direct` 的 terminfo 必须被正确配置。`alacritty` 的 terminfo 会在安装后自动被识别。

如果执行下列命令后没有返回任何错误信息，`alacritty` 的terminfo 则已经被安装:

```sh
infocmp alacritty
```

如果尚不存在，则可以使用以下方法进行全局安装:

```
sudo tic -xe alacritty,alacritty-direct extra/alacritty.info
```

### 添加桌面快捷方式

许多 Linux 和 BSD 发行版支持将应用程序添加到系统菜单来实现添加桌面快捷方式。这将为 Alacritty 提供一种快捷启动的途径。以下命令用于为 Alacritty 添加桌面快捷方式。

```sh
sudo cp target/release/alacritty /usr/local/bin # 或者位于 $PATH 环境变量中的其他位置
sudo cp extra/logo/alacritty-term.svg /usr/share/pixmaps/Alacritty.svg
sudo desktop-file-install extra/linux/Alacritty.desktop
sudo update-desktop-database
```

如果您在使用Alacritty的图标时遇到问题，可以将其替换为`extra/logo/compat` 目录中提供的预渲染 PNG 和简化的 SVG。

### 手册(man)

安装手册(man)需要额外的包 `gzip` 支持:

```sh
sudo mkdir -p /usr/local/share/man/man1
gzip -c extra/alacritty.man | sudo tee /usr/local/share/man/man1/alacritty.1.gz > /dev/null
gzip -c extra/alacritty-msg.man | sudo tee /usr/local/share/man/man1/alacritty-msg.1.gz > /dev/null
```

### Shell 补全

要获得 Alacritty 关键字和参数的自动补全，您可以根据您的 shell 安装相应的补全。

#### Zsh

要安装 zsh 的补全，您可以将 `extra/completions/_alacritty` 文件放在 `$fpath` 引用的任何目录中。

如果您还没有通过 `~/.zshrc` 添加这样的目录，您可以像这样添加一个：

```sh
mkdir -p ${ZDOTDIR:-~}/.zsh_functions
echo 'fpath+=${ZDOTDIR:-~}/.zsh_functions' >> ${ZDOTDIR:-~}/.zshrc
```

然后把补全文件复制到这个目录:

```sh
cp extra/completions/_alacritty ${ZDOTDIR:-~}/.zsh_functions/_alacritty
```

#### Bash

为 bash 安装自动补全，您可以在您的 `~/.bashrc` 文件中 `source` `extra/completions/alacritty.bash`。 

如果您不准备删除您的 Alacritty 源代码文件目录，您可以这样:

```sh
echo "source $(pwd)/extra/completions/alacritty.bash" >> ~/.bashrc
```

否则您应该将这个文件复制到 `~/.bash_completion` 目录，具体参照下面的命令:

```sh
mkdir -p ~/.bash_completion
cp extra/completions/alacritty.bash ~/.bash_completion/alacritty
echo "source ~/.bash_completion/alacritty" >> ~/.bashrc
```

#### Fish

为 fish 安装自动补全，请运行以下命令:

```
mkdir -p $fish_complete_path[1]
cp extra/completions/alacritty.fish $fish_complete_path[1]/alacritty.fish
```
