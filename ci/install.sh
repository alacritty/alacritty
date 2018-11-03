#!/bin/bash

# Add clippy for linting with nightly builds
if [ "$TRAVIS_RUST_VERSION" == "nightly" ]; then
    rustup component add clippy-preview
fi
