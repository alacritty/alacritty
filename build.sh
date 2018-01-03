#!/bin/bash

if [[ $VERSION == "nightly" ]]; then
    rustup run nightly cargo test --no-default-features --features "clippy"
else
    cargo test --no-default-features
fi
