#!/bin/bash

# Check if any command failed
error=false

# Run clippy checks
if [ "$CLIPPY" == "true" ]; then
    cargo clippy --all-features --all-targets
    exit
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
if [ "$TRAVIS_OS_NAME" == "windows" ]; then
    if [ -n "$TRAVIS_TAG" ]; then
        mkdir -p "./target/debug/deps"
        cp "./target/release/winpty-agent.exe" "./target/debug/deps"
    else
        cp "./target/debug/winpty-agent.exe" "./target/debug/deps"
    fi
    cargo test -p winpty || error=true
fi

if [ $error == "true" ]; then
    exit 1
fi
