#!/usr/bin/env bash

# The full path to the script directory, regardless of pwd.
DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# Current UNIX time.
TIME=$(date +%s)

# Make sure FlameGraph scripts are available.
if [ ! -e $DIR/FlameGraph ]
then
    git clone https://github.com/BrendanGregg/FlameGraph \
        $DIR/create-flamegraph/FlameGraph
fi

# Make sure a release build of Alacritty is available.
if [ ! -e $DIR/../target/release/alacritty ]
then
    echo "Must build alacritty first: cargo build --release"
    exit 1
fi

# Make sure perf is available.
if [ ! -x "$(command -v perf)" ]
then
    echo "Cannot find perf, please make sure it's installed"
    exit 1
fi

# Run perf, this will block while alacritty runs.
perf record -g -F 99 $DIR/../target/release/alacritty
perf script \
    | $DIR/create-flamegraph/FlameGraph/stackcollapse-perf.pl \
    | $DIR/create-flamegraph/FlameGraph/flamegraph.pl --width 1920 \
    > flame-$TIME.svg

# Tell users where the file is.
echo "Flame graph created at: file://$(pwd)/flame-$TIME.svg"
