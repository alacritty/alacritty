#!/bin/bash

# Add clippy for linting with nightly builds
if [ "$CLIPPY" == "true" ]; then
    rustup component add clippy
    rustup component add rustfmt
fi
