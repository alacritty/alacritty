#!/usr/bin/env sh
#
# File: alacritty desktop install
# Author: Colps
# Github: https://github.com/colpshift
# Description: install alacritty desktop
# Last Modified: 03/01/2021 20:12
#
sudo cp target/release/alacritty /usr/local/bin # or anywhere else in $PATH
sudo cp extra/logo/alacritty-term.svg /usr/share/pixmaps/Alacritty.svg
sudo desktop-file-install extra/linux/Alacritty.desktop
sudo update-desktop-database

