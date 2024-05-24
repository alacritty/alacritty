#!/usr/bin/env bash

# The full path to the script directory, regardless of pwd.
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# Make sure perf is available.
if [ ! -x "$(command -v perf)" ]
then
    echo "Cannot find perf, please make sure it's installed."
    exit 1
fi

# Install cargo-flamegraph
installed_flamegraph=0
if [ ! -x "$(command -v cargo-flamegraph)" ]; then
    echo "cargo-flamegraph not installed; installing ..."
    cargo install flamegraph
    installed_flamegraph=1
fi

# Create flamegraph
cargo flamegraph --bin=alacritty -- $@

# Uninstall cargo-flamegraph if it has been installed with this script
if [ $installed_flamegraph == 1 ]; then
    read -p "Would you like to uninstall cargo-flamegraph? [Y/n] " -n 1 -r
    echo
    if [[ "$REPLY" =~ ^[^Nn]*$ ]]; then
        cargo uninstall flamegraph
    fi
fi
