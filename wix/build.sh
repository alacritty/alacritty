#!/bin/bash

"$WIX/bin/candle.exe" -nologo -arch "x64" -ext WixUIExtension -ext WixUtilExtension -out "alacritty.wixobj" "alacritty.wxs"

"$WIX/bin/light.exe" -nologo -ext WixUIExtension -ext WixUtilExtension -out "alacritty.msi" -sice:ICE57 "alacritty.wixobj"
