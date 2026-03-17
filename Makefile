APP_NAME = ThroughWaves
APP_BUNDLE = $(APP_NAME).app
BINARY_NAME = jamhub-app
CARGO_TARGET_DIR = target

.PHONY: all build release bundle run clean

all: release bundle

# Debug build
build:
	cargo build

# Release build
release:
	cargo build --release

# Create / refresh the macOS .app bundle with the release binary
bundle: release
	@mkdir -p "$(APP_BUNDLE)/Contents/MacOS"
	@mkdir -p "$(APP_BUNDLE)/Contents/Resources"
	@cp "$(CARGO_TARGET_DIR)/release/$(BINARY_NAME)" "$(APP_BUNDLE)/Contents/MacOS/$(APP_NAME)"
	@chmod +x "$(APP_BUNDLE)/Contents/MacOS/$(APP_NAME)"
	@echo "Bundle ready: $(APP_BUNDLE)"
	@echo "  Double-click $(APP_BUNDLE) in Finder to launch."

# Build debug and copy into the bundle
bundle-debug: build
	@mkdir -p "$(APP_BUNDLE)/Contents/MacOS"
	@mkdir -p "$(APP_BUNDLE)/Contents/Resources"
	@cp "$(CARGO_TARGET_DIR)/debug/$(BINARY_NAME)" "$(APP_BUNDLE)/Contents/MacOS/$(APP_NAME)"
	@chmod +x "$(APP_BUNDLE)/Contents/MacOS/$(APP_NAME)"
	@echo "Debug bundle ready: $(APP_BUNDLE)"

# Launch the app bundle
run: bundle
	open "$(APP_BUNDLE)"

# Clean build artifacts (keeps the .app skeleton)
clean:
	cargo clean

# ---- App Icon ----
# To add an app icon:
#   1. Create a 1024x1024 PNG named icon.png in the repo root
#   2. Run: make icon
#   3. The icon will be embedded in the .app bundle
#
# Requires iconutil (ships with Xcode command line tools).
icon: icon.png
	@mkdir -p ThroughWaves.iconset
	sips -z 16 16     icon.png --out ThroughWaves.iconset/icon_16x16.png
	sips -z 32 32     icon.png --out ThroughWaves.iconset/icon_16x16@2x.png
	sips -z 32 32     icon.png --out ThroughWaves.iconset/icon_32x32.png
	sips -z 64 64     icon.png --out ThroughWaves.iconset/icon_32x32@2x.png
	sips -z 128 128   icon.png --out ThroughWaves.iconset/icon_128x128.png
	sips -z 256 256   icon.png --out ThroughWaves.iconset/icon_128x128@2x.png
	sips -z 256 256   icon.png --out ThroughWaves.iconset/icon_256x256.png
	sips -z 512 512   icon.png --out ThroughWaves.iconset/icon_256x256@2x.png
	sips -z 512 512   icon.png --out ThroughWaves.iconset/icon_512x512.png
	sips -z 1024 1024 icon.png --out ThroughWaves.iconset/icon_512x512@2x.png
	iconutil -c icns ThroughWaves.iconset -o "$(APP_BUNDLE)/Contents/Resources/ThroughWaves.icns"
	@rm -rf ThroughWaves.iconset
	@echo "Icon installed into $(APP_BUNDLE)/Contents/Resources/ThroughWaves.icns"
