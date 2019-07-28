#!/bin/bash

# All files which should be added only if they changed
aux_files=("extra/completions/alacritty.bash"
           "extra/completions/alacritty.fish"
           "extra/completions/_alacritty"
           "extra/linux/alacritty.desktop"
           "extra/alacritty.info"
           "alacritty.yml")

# Output binary name
name="Alacritty-${TRAVIS_TAG}"

# Everything in this directory will be offered as download for the release
mkdir "./target/deploy"

function windows {
    choco install 7zip nuget.commandline
    nuget install WiX

    # Create zip archive
    7z a -tzip "./target/deploy/${name}-windows-portable.zip" "./target/release/alacritty.exe" \
        "./target/release/winpty-agent.exe"

    # Create msi installer
    ./WiX.*/tools/candle.exe -nologo -arch "x64" -ext WixUIExtension -ext WixUtilExtension -out \
        "target/alacritty.wixobj" "extra/windows/wix/alacritty.wxs"
    ./WiX.*/tools/light.exe -nologo -ext WixUIExtension -ext WixUtilExtension -out \
        "target/installer.msi" -sice:ICE61 -sice:ICE91 "target/alacritty.wixobj"
    mv "target/installer.msi" "target/deploy/${name}-windows-installer.msi"
}

function osx {
    rm -rf "./target/release" \
        && make dmg \
        && mv "./target/release/osx/Alacritty.dmg" "./target/deploy/${name}.dmg"
}

function debian {
    arch=$1

    docker pull "undeadleech/alacritty-ubuntu-${arch}" \
        && docker_tar "alacritty-ubuntu-${arch}" "ubuntu_18_04_${arch}" \
        && docker_deb "alacritty-ubuntu-${arch}" "ubuntu_18_04_${arch}" \
        && sudo chown -R $USER:$USER "./target"
}

function docker_tar {
    image=$1
    archname=$2

    docker run -v "$(pwd):/source" "undeadleech/${image}" \
        /root/.cargo/bin/cargo build --release --manifest-path /source/Cargo.toml

    tar -cvzf "./target/deploy/${name}-${archname}.tar.gz" -C "./target/release/" "alacritty"
}

function docker_deb {
    image=$1
    archname=$2

    docker run -v "$(pwd):/source" "undeadleech/${image}" sh -c \
        "cd /source && /root/.cargo/bin/cargo deb --no-build --manifest-path alacritty/Cargo.toml \
        --output ./target/deploy/${name}-${archname}.deb"
}

if [ "$TRAVIS_OS_NAME" == "osx" ]; then
    osx || exit
elif [ "$TRAVIS_OS_NAME" == "linux" ] && [ "$ARCH" != "i386" ]; then
    debian "amd64" || exit
elif [ "$TRAVIS_OS_NAME" == "linux" ] && [ "$ARCH" == "i386" ]; then
    debian "i386" || exit
elif [ "$TRAVIS_OS_NAME" == "windows" ]; then
    windows
fi

# Convert and add manpage if it changed
gzip -c "./extra/alacritty.man" > "./target/deploy/alacritty.1.gz" || exit

# Rename Alacritty logo to match .desktop file
cp "./extra/logo/alacritty-term.svg" "./target/deploy/Alacritty.svg" || exit

# Offer various other files
for file in "${aux_files[@]}"; do
    cp $file "./target/deploy/" || exit
done
