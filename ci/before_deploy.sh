#!/bin/bash

# All files which should be added only if they changed
aux_files=("alacritty-completions.bash"
           "alacritty-completions.fish"
           "alacritty-completions.zsh"
           "alacritty.desktop"
           "alacritty.info"
           "alacritty.yml"
           "alacritty_macos.yml"
           "alacritty_windows.yml")

# Get previous tag to check for changes
git fetch --tags
git fetch --unshallow
prev_tag=$(git describe --tags --abbrev=0 $TRAVIS_TAG^)

# Everything in this directory will be offered as download for the release
mkdir "./target/deploy"

# Create macOS binary
if [ "$TRAVIS_OS_NAME" == "osx" ]; then
  make dmg;
  mv "./target/release/osx/Alacritty.dmg" "./target/deploy/Alacritty-${TRAVIS_TAG}.dmg";
fi

# Create Linux .deb binary
if [ "$TRAVIS_OS_NAME" == "linux" ]; then
  cargo install cargo-deb;
  DEB=$(cargo deb --no-build);
  mv "$DEB" "./target/deploy/Alacritty-${TRAVIS_TAG}_amd64.deb";
fi

# Create windows binary
if [ "$TRAVIS_OS_NAME" == "windows" ]; then
  mv "./target/release/alacritty.exe" "./target/deploy/Alacritty-${TRAVIS_TAG}.exe";
  mv "./target/release/winpty-agent.exe" "./target/deploy/winpty-agent.exe";
fi

# Convert and add manpage if it changed
if [ -n "$(git diff $prev_tag HEAD alacritty.man)" ]; then
    gzip -z "./alacritty.man" > "./target/deploy/alacritty.1.gz"
fi

# Offer extra files if they changed
for file in "${aux_files[@]}"; do
    if [ -n "$(git diff $prev_tag HEAD $file)" ]; then
        cp $file "./target/deploy/"
    fi
done
