#!/usr/bin/bash

if [[ $VERSION == "nightly" ]]; then
    cargo test --no-default-features --features "clippy"
else
    cargo test --no-default-features
fi
