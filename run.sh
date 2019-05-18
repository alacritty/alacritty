#!/bin/bash
set -ex

sudo rm -rf alacritty/.rpm
docker build -f ci/x86_64/fedora/Dockerfile -t fedora-rust .

docker run -v "$(pwd):/source:z" fedora-rust \
     sh -c "cargo rpm init && \
     cargo build --release && \
     cargo rpm build"
