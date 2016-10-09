#!/usr/bin/env bash

# Make sure FlameGraph scripts are available
if [ ! -e ./FlameGraph ]
then
    git clone https://github.com/BrendanGregg/FlameGraph
fi

if [ ! -e target/release/alacritty ]
then
    echo "Must build alacritty first: cargo build --release"
    exit 1
fi

# This will block while alacritty runs
perf record -g -F 99 target/release/alacritty
perf script | ./FlameGraph/stackcollapse-perf.pl | ./FlameGraph/flamegraph.pl --width 1920 > alacritty.svg

echo "Flame graph created at file://$(pwd)/alacritty.svg"
