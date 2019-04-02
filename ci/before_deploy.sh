#!/bin/bash

# All files which should be added only if they changed
aux_files=("extra/completions/alacritty.bash"
           "extra/completions/alacritty.fish"
           "extra/completions/_alacritty"
           "extra/linux/alacritty.desktop"
           "extra/alacritty.info"
           "alacritty.yml")

# Get previous tag to check for changes
git fetch --tags
git fetch --unshallow
prev_tag=$(git describe --tags --abbrev=0 $TRAVIS_TAG^)

# Everything in this directory will be offered as download for the release
mkdir "./target/deploy"

# Output binary name
name="Alacritty-${TRAVIS_TAG}"

if [ "$TRAVIS_OS_NAME" == "osx" ]; then
    rm -rf "./target/release"
    make dmg
    mv "./target/release/osx/Alacritty.dmg" "./target/deploy/${name}.dmg"
elif [ "$TRAVIS_OS_NAME" == "linux" ] && [ "$ARCH" != "i386" ]; then
    docker pull undeadleech/alacritty-ubuntu

    # x86_64
    docker run -v "$(pwd):/source" undeadleech/alacritty-ubuntu \
        /root/.cargo/bin/cargo build --release --manifest-path /source/Cargo.toml
    tar -cvzf "./target/deploy/${name}-x86_64.tar.gz" -C "./target/release/" "alacritty"

    # x86_64 deb
    docker run -v "$(pwd):/source" undeadleech/alacritty-ubuntu \
        sh -c "cd /source && \
        /root/.cargo/bin/cargo deb --no-build --output ./target/deploy/${name}_amd64.deb"

    # Make sure all files can be uploaded without permission errors
    sudo chown -R $USER:$USER "./target"
elif [ "$TRAVIS_OS_NAME" == "linux" ] && [ "$ARCH" == "i386" ]; then
    docker pull undeadleech/alacritty-ubuntu-i386

    # i386
    docker run -v "$(pwd):/source" undeadleech/alacritty-ubuntu-i386 \
        /root/.cargo/bin/cargo build --release --manifest-path /source/Cargo.toml
    tar -cvzf "./target/deploy/${name}-i386.tar.gz" -C "./target/release/" "alacritty"

    # i386 deb
    docker run -v "$(pwd):/source" undeadleech/alacritty-ubuntu-i386 \
        sh -c "cd /source && \
        /root/.cargo/bin/cargo deb --no-build --output ./target/deploy/${name}_i386.deb"

    # Make sure all files can be uploaded without permission errors
    sudo chown -R $USER:$USER "./target"
elif [ "$TRAVIS_OS_NAME" == "windows" ]; then
    choco install 7zip nuget.commandline
    nuget install WiX

    # Create zip archive
    7z a -tzip "./target/deploy/${name}-windows-portable.zip" "./target/release/alacritty.exe" \
        "./target/release/winpty-agent.exe"

    # Create msi installer
    ./WiX.*/tools/candle.exe -nologo -arch "x64" -ext WixUIExtension -ext WixUtilExtension -out "target/alacritty.wixobj" "extra/windows/wix/alacritty.wxs"
    ./WiX.*/tools/light.exe -nologo -ext WixUIExtension -ext WixUtilExtension -out "target/installer.msi" -sice:ICE61 -sice:ICE91 "target/alacritty.wixobj"
    mv "target/installer.msi" "target/deploy/${name}-windows-installer.msi"
fi

# Convert and add manpage if it changed
if [ -n "$(git diff $prev_tag HEAD extra/alacritty.man)" ]; then
    gzip -c "./extra/alacritty.man" > "./target/deploy/alacritty.1.gz"
fi

# Offer various other files
for file in "${aux_files[@]}"; do
    cp $file "./target/deploy/"
done
