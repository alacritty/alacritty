#! /bin/sh

scdoc < ./alacritty/extra/man/alacritty-bindings.5.scd >./target/debian/alacritty-bindings.5
scdoc < ./alacritty/extra/man/alacritty-msg.1.scd >./target/debian/alacritty-msg.1
scdoc < ./alacritty/extra/man/alacritty.1.scd >./target/debian/alacritty.1
scdoc < ./alacritty/extra/man/alacritty.5.scd >./target/debian/alacritty.5

cd ./target/debian/
gzip --no-name --best alacritty-bindings.5
gzip --no-name --best alacritty-msg.1
gzip --no-name --best alacritty.1
gzip --no-name --best alacritty.5
cd ../../alacritty/

cargo deb