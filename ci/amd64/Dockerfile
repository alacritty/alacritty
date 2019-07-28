FROM ubuntu:latest

ENV USER root

RUN apt-get update && apt-get install -y cmake libfreetype6-dev libfontconfig1-dev curl python3 \
        libxcb-xfixes0-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
RUN /root/.cargo/bin/cargo install cargo-deb
