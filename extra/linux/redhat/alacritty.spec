Name:          alacritty
Version:       0.4.1-rc3
Release:       1%{?dist}
Summary:       A cross-platform, GPU enhanced terminal emulator
License:       ASL 2.0
URL:           https://github.com/jwilm/alacritty
VCS:           https://github.com/jwilm/alacritty.git
Source:        alacritty-%{version}.tar

BuildRequires: rust >= 1.36.0
BuildRequires: cargo
BuildRequires: cmake
BuildRequires: freetype-devel
BuildRequires: fontconfig-devel
BuildRequires: libxcb-devel
BuildRequires: desktop-file-utils
BuildRequires: python36

%description
Alacritty is a terminal emulator with a strong focus on simplicity and
performance. With such a strong focus on performance, included features are
carefully considered and you can always expect Alacritty to be blazingly fast.
By making sane choices for defaults, Alacritty requires no additional setup.
However, it does allow configuration of many aspects of the terminal.

%prep
%setup -q -n alacritty-%{version}

%build
cargo build --release

%install
install -p -D -m755 target/release/alacritty         %{buildroot}%{_bindir}/alacritty
install -p -D -m644 extra/linux/alacritty.desktop    %{buildroot}%{_datadir}/applications/alacritty.desktop
install -p -D -m644 extra/logo/alacritty-term.svg    %{buildroot}%{_datadir}/pixmaps/Alacritty.svg
install -p -D -m644 alacritty.yml                    %{buildroot}%{_datadir}/alacritty/alacritty.yml
tic     -xe alacritty,alacritty-direct \
                    extra/alacritty.info       -o    %{buildroot}%{_datadir}/terminfo
install -p -D -m644 extra/completions/alacritty.bash %{buildroot}%{_datadir}/bash-completion/completions/alacritty
install -p -D -m644 extra/completions/_alacritty     %{buildroot}%{_datadir}/zsh/site-functions/_alacritty
install -p -D -m644 extra/completions/alacritty.fish %{buildroot}%{_datadir}/fish/vendor_completions.d/alacritty.fish
install -p -D -m644 extra/alacritty.man              %{buildroot}%{_mandir}/man1/alacritty.1

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/alacritty.desktop

%files
%{_bindir}/alacritty
%{_datadir}/applications/alacritty.desktop
%{_datadir}/pixmaps/Alacritty.svg
%{_datadir}/alacritty/alacritty.yml
%{_datadir}/terminfo/a/alacritty*
%{_datadir}/bash-completion/completions/alacritty
%{_datadir}/zsh/site-functions/_alacritty
%{_datadir}/fish/vendor_completions.d/alacritty.fish
%{_mandir}/man1/alacritty.1*
