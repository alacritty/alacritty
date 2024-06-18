#!/usr/bin/env bash

set -e

usage() {
>&2 cat << EOF
usage: $0 [-bhp]
-b <wayland|x11> select the rendering backend
-h               usage information
-p <prefix>      select the installation prefix (defaults to '/usr/local')
EOF
}

opt_failed=false
opt_wayland=false
opt_x11=false
opt_prefix='/usr/local'

while getopts 'b:hp:' opt_name; do
  case ${opt_name} in
    h)
      usage
      exit 0
      ;;
    b)
      if [[ "wayland" == $OPTARG ]]; then
        opt_wayland=true
      elif [[ "x11" == $OPTARG ]]; then
        opt_x11=true
      else
        opt_failed=true
      fi
      ;;
    p)
      if [[ -d $OPTARG ]]; then
        opt_prefix=$OPTARG
      else
        opt_failed=true
      fi
      ;;
    ?)
      opt_failed=true
      ;;
  esac

  if $opt_failed; then
    usage
    exit 1
  fi
done

cargo clean
if $opt_wayland; then
  echo "alacritty: wayland release build"
  cargo b --release --no-default-features --features=wayland
elif $opt_x11; then
  echo "alacritty: x11 release build"
  cargo b --release --no-default-features --features=x11
else
  echo "alacritty: default release build"
  cargo b --release
fi

echo "alacritty: install"
sudo cp -v target/release/alacritty "${opt_prefix}/bin"
# FIXME: use ${opt_prefix} in the future:
sudo cp -v extra/logo/alacritty-term.svg "/usr/share/pixmaps/Alacritty.svg"
sudo desktop-file-install extra/linux/Alacritty.desktop
sudo update-desktop-database
