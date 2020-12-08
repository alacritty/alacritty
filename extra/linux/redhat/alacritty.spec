Name:          alacritty
Version:       0.7.0-dev
Release:       1%{?dist}
Summary:       A cross-platform, GPU enhanced terminal emulator
License:       ASL 2.0
URL:           https://github.com/alacritty/alacritty
VCS:           https://github.com/alacritty/alacritty.git
Source:        alacritty-%{version}.tar

%if 0%{?fedora} >= 33
Requires: ncurses-base
%endif
Requires: freetype
Requires: fontconfig
Requires: libxcb

BuildRequires: rust >= 1.43.0
BuildRequires: cargo
BuildRequires: cmake
BuildRequires: gcc-c++
BuildRequires: python3
BuildRequires: freetype-devel
BuildRequires: fontconfig-devel
BuildRequires: libxcb-devel
BuildRequires: desktop-file-utils
BuildRequires: ncurses

%description
Alacritty is the fastest terminal emulator in existence.

%prep
%setup -q -n alacritty-%{version}

%build
cargo build --release

%install
install -p -D -m755 target/release/alacritty         %{buildroot}%{_bindir}/alacritty
install -p -D -m644 extra/linux/Alacritty.desktop    %{buildroot}%{_datadir}/applications/Alacritty.desktop
install -p -D -m644 extra/logo/alacritty-term.svg    %{buildroot}%{_datadir}/pixmaps/Alacritty.svg
install -p -D -m644 alacritty.yml                    %{buildroot}%{_datadir}/alacritty/alacritty.yml
%if 0%{?fedora} < 33
tic     -xe alacritty,alacritty-direct \
                    extra/alacritty.info       -o    %{buildroot}%{_datadir}/terminfo
%endif
install -p -D -m644 extra/completions/alacritty.bash %{buildroot}%{_datadir}/bash-completion/completions/alacritty
install -p -D -m644 extra/completions/_alacritty     %{buildroot}%{_datadir}/zsh/site-functions/_alacritty
install -p -D -m644 extra/completions/alacritty.fish %{buildroot}%{_datadir}/fish/vendor_completions.d/alacritty.fish
install -p -D -m644 extra/alacritty.man              %{buildroot}%{_mandir}/man1/alacritty.1

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/Alacritty.desktop

%files
%{_bindir}/alacritty
%{_datadir}/applications/Alacritty.desktop
%{_datadir}/pixmaps/Alacritty.svg
%dir %{_datadir}/alacritty/
%{_datadir}/alacritty/alacritty.yml
%if 0%{?fedora} < 33
%{_datadir}/terminfo/a/alacritty*
%endif
%{_datadir}/bash-completion/completions/alacritty
%{_datadir}/zsh/site-functions/_alacritty
%{_datadir}/fish/vendor_completions.d/alacritty.fish
%{_mandir}/man1/alacritty.1*
