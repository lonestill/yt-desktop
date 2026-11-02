#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="YouTube.app"
APP_DIR="$DIR/$APP_NAME"
CONTENTS="$APP_DIR/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

echo "[*] Building tpws zapret engine..."
cd "$DIR/tpws"
cc -std=gnu99 -Imacos -Iepoll-shim/include -o tpws *.c epoll-shim/src/*.c -lz -lpthread
strip tpws

echo "[*] Building release binary..."
cd "$DIR"
cargo build --release

echo "[*] Assembling macOS app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS" "$RESOURCES"

cp "$DIR/target/release/yt-desktop" "$MACOS/yt-desktop"
cp "$DIR/tpws/tpws" "$MACOS/tpws"
chmod +x "$MACOS/yt-desktop" "$MACOS/tpws"

if [ -f "/Users/a123/Projects/ytm-desktop/src-tauri/icons/icon.icns" ]; then
    cp "/Users/a123/Projects/ytm-desktop/src-tauri/icons/icon.icns" "$RESOURCES/AppIcon.icns"
fi

cat << 'EOF' > "$CONTENTS/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>YouTube</string>
    <key>CFBundleExecutable</key>
    <string>yt-desktop</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.yt.desktop</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>YouTube</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSAppTransportSecurity</key>
    <dict>
        <key>NSAllowsArbitraryLoads</key>
        <true/>
    </dict>
</dict>
</plist>
EOF

echo "[*] Signing app bundle..."
codesign --force --deep -s - "$APP_DIR"

echo "[*] YouTube.app successfully built at $APP_DIR"
