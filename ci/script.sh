#!/bin/bash

# Check if any command failed
error=false

# Run clippy on nightly builds
if [ "$TRAVIS_RUST_VERSION" == "nightly" ]; then
    cargo clippy --all-features --all-targets || error=true
fi

# Run test in release mode if a tag is present, to produce an optimized binary
if [ -n "$TRAVIS_TAG" ]; then
    cargo test --release || error=true
else
    cargo test || error=true
fi

# Test the font subcrate
cargo test -p font || error=true

# Test the winpty subcrate
if [ "$TRAVIS_OS_NAME" == "windows"]; then
    cp ./target/debug/winpty-agent.exe ./target/debug/deps && \
        cargo test -p winpty || error=true
fi

if [ $error == "true" ]; then
    exit 1
fi
