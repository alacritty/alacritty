#!/bin/bash

# Switch from GNU to MSVC toolchain on Windows
if [ "$TRAVIS_OS_NAME" == "windows" ]; then
    rustup default "${TRAVIS_RUST_VERSION}-x86_64-pc-windows-msvc"
fi

# Add clippy for lint validation
if [ "$CLIPPY" == "true" ]; then
    rustup component add clippy
fi

# Add rustfmt for format validation
if [ "$RUSTFMT" == "true" ]; then
    rustup component add rustfmt
fi
