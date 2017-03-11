# justfile
# see: https://github.com/casey/just

APP_BINARY     = 'target/release/alacritty'
APP_BINARY_DIR = DST_DIR + '/' + APP_NAME + '/Contents/MacOS'
APP_NAME       = 'Alacritty.app'
DMG_NAME       = 'Alacritty.dmg'
DMG_PATH       = DST_DIR + '/' + DMG_NAME
DST_DIR        = 'target/release/macos'

default:
	just --list

# Build release binary with cargo
build-release:
	cargo build --release

# Clone Alacritty.app template and mount binary
macos-app: build-release
	@mkdir -p '{{APP_BINARY_DIR}}'
	@cp -fRp 'assets/macos/{{APP_NAME}}' '{{DST_DIR}}'
	@cp -fp '{{APP_BINARY}}' '{{APP_BINARY_DIR}}'
	@echo "Created '{{APP_NAME}}' in '{{DST_DIR}}'"

# Pack Alacritty.app into .dmg
macos-dmg: macos-app
	@echo "Packing disk image..."
	@hdiutil create '{{DMG_PATH}}' \
		-volname "Alacritty" \
		-fs HFS+ \
		-srcfolder '{{DST_DIR}}' \
		-ov -format UDZO
	@echo "Packed '{{DMG_NAME}}' in '{{DST_DIR}}'"

# Mount disk image
macos-install: macos-dmg
	@open '{{DMG_PATH}}'

# Remove all artifacts
macos-clean:
	rm -rf '{{DST_DIR}}'

# Local Variables:
# mode: makefile
# End:

# vim: set ft=make :
