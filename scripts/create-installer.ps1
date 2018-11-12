$rootDir = Resolve-Path "$PSScriptRoot/.."
$tools = "$rootDir/scripts/squirrel"

$targetBuildDir = "$rootDir/target/installer"
$releaseDir = "$targetBuildDir/release"

& git clean -xdf "$targetBuildDir"
mkdir "$targetBuildDir"

& "$tools/nuget.exe" pack "$rootDir/alacritty.nuspec" -BasePath "$rootDir" -OutputDirectory "$targetBuildDir"

$nugetFile = Get-Item "$targetBuildDir/*.nupkg"
& "$tools/squirrel.com" --releasify $nugetFile -r "$releaseDir" --no-msi