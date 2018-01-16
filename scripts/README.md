Scripts
=======

## Flamegraph

Run the release version of Alacritty while recording call stacks. After the
Alacritty process exits, a flamegraph will be generated and it's URI printed
as the only output to STDOUT.

```sh
./create-flamegraph.sh
```

Running this script depends on an installation of `perf`. Running the included
`create-flamegraph/ubuntu-install-perf.sh` takes care of this on Ubuntu. For
other operating systems refer to thier package manager.

## ANSI Color Tests

We include a few scripts for testing the color of text inside a terminal. The
first shows various foreground and background varients. The second enumerates
all the colors of a standard terminal. The third enumerates the 24-bit colors.

```sh
./fg-bg.sh
./colors.sh
./24-bit-colors.sh
```
