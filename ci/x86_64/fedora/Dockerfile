FROM fedora:latest

ENV USER root

RUN dnf install --assumeyes \
    cargo \
    cmake \
    fontconfig-devel \
    freetype-devel \
    libxcb-devel \
    rpm-build \
    rust

RUN cargo install cargo-rpm

WORKDIR source/alacritty
