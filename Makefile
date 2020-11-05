TARGET = alacritty

ASSETS_DIR = extra
RELEASE_DIR = target/release
MANPAGE = $(ASSETS_DIR)/alacritty.man
TERMINFO = $(ASSETS_DIR)/alacritty.info
COMPLETIONS_DIR = $(ASSETS_DIR)/completions
COMPLETIONS = $(COMPLETIONS_DIR)/_alacritty \
	$(COMPLETIONS_DIR)/alacritty.bash \
	$(COMPLETIONS_DIR)/alacritty.fish

APP_NAME = Alacritty.app
APP_TEMPLATE = $(ASSETS_DIR)/osx/$(APP_NAME)
APP_DIR = $(RELEASE_DIR)/osx
APP_BINARY = $(RELEASE_DIR)/$(TARGET)
APP_BINARY_DIR = $(APP_DIR)/$(APP_NAME)/Contents/MacOS
APP_EXTRAS_DIR = $(APP_DIR)/$(APP_NAME)/Contents/Resources
APP_COMPLETIONS_DIR = $(APP_EXTRAS_DIR)/completions

DMG_NAME = Alacritty.dmg
DMG_DIR = $(RELEASE_DIR)/osx

vpath $(TARGET) $(RELEASE_DIR)
vpath $(APP_NAME) $(APP_DIR)
vpath $(DMG_NAME) $(APP_DIR)

all: help

help: ## Prints help for targets with comments
	@grep -E '^[a-zA-Z._-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

binary: | $(TARGET) ## Build release binary with cargo
$(TARGET):
	MACOSX_DEPLOYMENT_TARGET="10.11" cargo build --release

app: | $(APP_NAME) ## Clone Alacritty.app template and mount binary
$(APP_NAME): $(TARGET)
	@mkdir -p $(APP_BINARY_DIR)
	@mkdir -p $(APP_EXTRAS_DIR)
	@mkdir -p $(APP_COMPLETIONS_DIR)
	@gzip -c $(MANPAGE) > $(APP_EXTRAS_DIR)/alacritty.1.gz
	@tic -xe alacritty,alacritty-direct -o $(APP_EXTRAS_DIR) $(TERMINFO)
	@cp -fRp $(APP_TEMPLATE) $(APP_DIR)
	@cp -fp $(APP_BINARY) $(APP_BINARY_DIR)
	@cp -fp $(COMPLETIONS) $(APP_COMPLETIONS_DIR)
	@touch -r "$(APP_BINARY)" "$(APP_DIR)/$(APP_NAME)"
	@echo "Created '$@' in '$(APP_DIR)'"

dmg: | $(DMG_NAME) ## Pack Alacritty.app into .dmg
$(DMG_NAME): $(APP_NAME)
	@echo "Packing disk image..."
	@ln -sf /Applications $(DMG_DIR)/Applications
	@hdiutil create $(DMG_DIR)/$(DMG_NAME) \
		-volname "Alacritty" \
		-fs HFS+ \
		-srcfolder $(APP_DIR) \
		-ov -format UDZO
	@echo "Packed '$@' in '$(APP_DIR)'"

install: $(DMG_NAME) ## Mount disk image
	@open $(DMG_DIR)/$(DMG_NAME)

.PHONY: app binary clean dmg install $(TARGET)

clean: ## Remove all artifacts
	-rm -rf $(APP_DIR)
