FROM rust

RUN apt-get update && \
    apt-get install -y cmake pkg-config libfreetype6-dev libfontconfig1-dev libxcb-xfixes0-dev python3

WORKDIR /code

ADD . .

RUN RUSTFLAGS='-C link-arg=-s' cargo build --release

