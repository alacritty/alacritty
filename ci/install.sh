#!/bin/bash

# Add clippy for lint validation
if [ "$CLIPPY" == "true" ]; then
    rustup component add clippy
fi
