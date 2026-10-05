#!/bin/bash

set -xeuo pipefail

file_path=$1

tag=$(git describe --tags --abbrev=0)

if ! gh release view "$tag"
then
    gh release create --draft "$tag"
fi

gh release upload "$tag" "$file_path"
