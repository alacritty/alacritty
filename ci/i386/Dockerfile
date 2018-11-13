FROM i386/ubuntu:latest

ENV USER root

RUN apt-get update && apt-get install -y cmake libfreetype6-dev libfontconfig1-dev curl

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
RUN /root/.cargo/bin/rustup default stable-i686-unknown-linux-gnu
RUN /root/.cargo/bin/cargo install cargo-deb
