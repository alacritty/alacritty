FROM fedora:latest

ENV USER root

RUN dnf install --assumeyes cmake freetype-devel fontconfig-devel libxcb-devel

RUN dnf install --assumeyes rust cargo
RUN cargo install cargo-rpm

WORKDIR source/alacritty
