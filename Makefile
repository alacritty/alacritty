TARGET = alacritty

APP_NAME = Alacritty.app
ASSETS_DIR = assets
RELEASE_DIR = target/release
BINARY_FILE = $(RELEASE_DIR)/$(TARGET)
APP_TEMPLATE = $(ASSETS_DIR)/osx/$(APP_NAME)
APP_DIR = $(RELEASE_DIR)/osx
APP_BINARY_DIR  = $(APP_DIR)/$(APP_NAME)/Contents/MacOS


DMG_NAME = Alacritty.dmg
DMG_DIR = $(RELEASE_DIR)/osx

vpath $(TARGET) $(RELEASE_DIR)
vpath $(APP_NAME) $(APP_DIR)

all: help

help: ## Prints help for targets with comments
	@grep -E '^[a-zA-Z._-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

binary: | $(TARGET) ## Build release binary
$(TARGET):
	@echo "Please build '$@' with 'cargo build --release'"

app: | $(APP_NAME) ## Clone Alacritty.app template and mount binary
$(APP_NAME): $(TARGET) $(APP_TEMPLATE)
	@mkdir -p $(APP_DIR)
	@cp -R $(APP_TEMPLATE) $(APP_DIR)
	@mkdir $(APP_BINARY_DIR)
	@cp $(BINARY_FILE) $(APP_BINARY_DIR)
	@echo "$@ created in $(APP_DIR)"

dmg: | $(DMG_NAME) ## Pack Alacritty.app into .dmg
$(DMG_NAME): $(APP_NAME)
	@echo "Packing disk image..."
	@hdiutil create $(DMG_DIR)/$(DMG_NAME) \
		-volname "Alacritty" \
		-fs HFS+ \
		-srcfolder $(APP_DIR) \
		-ov -format UDZO
	@echo "$@ packed in $(APP_DIR)"

.PHONY: app binary clean dmg

clean: ## Remove all artifacts
	-rm -rf $(APP_DIR)
