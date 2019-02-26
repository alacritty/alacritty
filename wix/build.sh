#!/bin/bash

WiX.*/tools/candle.exe -nologo -arch "x64" -ext WixUIExtension -ext WixUtilExtension -out "wix/alacritty.wixobj" "wix/alacritty.wxs"

WiX.*/tools/light.exe -nologo -ext WixUIExtension -ext WixUtilExtension -out "wix/alacritty.msi" -sice:ICE61 -sice:ICE91 "wix/alacritty.wixobj"
