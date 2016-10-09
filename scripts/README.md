scripts
=======

There are two scripts included at the time this README was written, and they
both support flamegraph generation on Ubuntu. The first script installs the
required dependencies:

```sh
scripts/ubuntu-install-perf.sh
```

The second script will run Alacritty while recording call stacks. After the
Alacritty process exits, a flamegraph will be generated  and its URI printed.

```sh
scripts/create-flamegraph.sh
```

**NOTE**: The _create-flamegraph.sh_ script is intended to be run from the
alacritty project root.
