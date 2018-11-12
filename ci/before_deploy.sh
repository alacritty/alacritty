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

# Output binary name
name="Alacritty-${TRAVIS_TAG}"

if [ "$TRAVIS_OS_NAME" == "osx" ]; then
    make dmg
    mv "./target/release/osx/Alacritty.dmg" "./target/deploy/${name}.dmg"
elif [ "$TRAVIS_OS_NAME" == "linux" ]; then
    cargo install cargo-deb

    # x86_64
    docker pull undeadleech/alacritty-ubuntu
    docker run -v "$(pwd):/source" undeadleech/alacritty-ubuntu \
        /root/.cargo/bin/cargo build --release --manifest-path /source/Cargo.toml
    sudo chown -R $USER:$USER "./target"
    tar -cvzf "./target/deploy/${name}-x86_64.tar.gz" -C "./target/release/" "alacritty"

    # x86_64 deb
    DEB=$(cargo deb --no-build)
    mv "$DEB" "./target/deploy/${name}_amd64.deb"

    rm -rf "./target/release"

    # i386
    docker pull undeadleech/alacritty-ubuntu-i386
    docker run -v "$(pwd):/source" undeadleech/alacritty-ubuntu \
        /root/.cargo/bin/cargo build --release --manifest-path /source/Cargo.toml
    sudo chown -R $USER:$USER "./target"
    tar -cvzf "./target/deploy/${name}-i386.tar.gz" -C "./target/release/" "alacritty"

    # i386 deb
    DEB=$(cargo deb --no-build)
    mv "$DEB" "./target/deploy/${name}_i386.deb"
elif [ "$TRAVIS_OS_NAME" == "windows" ]; then
    mv "./target/release/alacritty.exe" "./target/deploy/${name}.exe"
    mv "./target/release/winpty-agent.exe" "./target/deploy/winpty-agent.exe"
fi

# Convert and add manpage if it changed
if [ -n "$(git diff $prev_tag HEAD alacritty.man)" ]; then
    gzip -c "./alacritty.man" > "./target/deploy/alacritty.1.gz"
fi

# Offer extra files if they changed
for file in "${aux_files[@]}"; do
    if [ -n "$(git diff $prev_tag HEAD $file)" ]; then
        cp $file "./target/deploy/"
    fi
done
