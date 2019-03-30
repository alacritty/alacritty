#!/bin/bash

# Add clippy for lint validation
if [ "$CLIPPY" == "true" ]; then
    rustup component add clippy
fi

# Add rustfmt for format validation
if [ "$RUSTFMT" == "true" ]; then
    rustup component add rustfmt
fi
