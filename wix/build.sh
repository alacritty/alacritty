#!/bin/bash

"WiX.3.11.1/tools/candle.exe" -nologo -arch "x64" -ext WixUIExtension -ext WixUtilExtension -out "alacritty.wixobj" "alacritty.wxs"

"WiX.3.11.1/tools/light.exe" -nologo -ext WixUIExtension -ext WixUtilExtension -out "alacritty.msi" -sice:ICE57 "alacritty.wixobj"
