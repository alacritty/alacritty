#!/bin/bash
set -eu
[[ -n ${DEBUG:-} ]] && set -x

#### Example alacritty.yml usage
#key_bindings:
#  - { key: N,  mods: Control|Shift,  action: SpawnNewInstance }
#  - { key: O,  mods: Control|Shift,  command: { program: "opacity-change.sh", args: ["-"] } }
#  - { key: P,  mods: Control|Shift,  command: { program: "opacity-change.sh", args: ["+"] } }


operation="${1:-}${2:-}"      # Arg #1 & #2 (in case the user misinterpreted a space in the usage), Default ''
step="${operation:1}"         # Substring from char index 1
step="${step:-1}"             # Default '1'
operation="${operation:0:1}"  # Substring from char index 0 length of 1
config_file="$HOME/.config/alacritty/alacritty.yml"
config_field="opacity"
tmp_file="/tmp/$(basename $config_file).$(date +%s)"
current_value=$(sed 's/#.*//g; /\b'"$config_field"':/!d; s/.*: \?//' < "$config_file")

case $operation in
"-")
    verb="Decreasing" ;;
"+")
    verb="Increasing" ;;
*)
    echo "Usage: ${BASH_SOURCE[0]} (-|+)[int]"; exit 255 ;;
esac

new_value="$(awk '{n=$1+$2/10; print (n<0 ? 0 : n>1 ? 1 : n)}' <<<"$current_value $operation$step")"
echo "$verb $config_field from $current_value to $new_value" >&2
cp "$config_file" "$tmp_file"
sed "s/\b$config_field:.*/$config_field: $new_value/" "$tmp_file" > "$config_file"
