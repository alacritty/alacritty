#!/bin/bash

# Run clippy checks
if [ "$CLIPPY" == "true" ]; then
    cargo clippy --all-targets
    exit
fi

# Run test in release mode if a tag is present, to produce an optimized binary
if [ -n "$TRAVIS_TAG" ]; then
    # Build separately so we generate an 'alacritty' binary without -HASH appended
    cargo build --release
    cargo test --release
else
    cargo test
fi
