#!/usr/bin/env bash
set -v

# Get kernel info
UNAME=$(uname -r)

# Install linux tools for the perf binary
sudo apt-get install -y \
    linux-tools-common \
    linux-tools-generic \
    linux-tools-$UNAME
