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
if command -v make >/dev/null 2>&1; then
    make mac || {
        cc -std=gnu99 -Imacos -Iepoll-shim/include -o tpws *.c epoll-shim/src/*.c -lz -lpthread
        strip tpws
    }
else
    cc -std=gnu99 -Imacos -Iepoll-shim/include -o tpws *.c epoll-shim/src/*.c -lz -lpthread
    strip tpws
fi

echo "[*] Building release binary..."
cd "$DIR"
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
mkdir -p target/universal/release
lipo -create target/aarch64-apple-darwin/release/yt-desktop target/x86_64-apple-darwin/release/yt-desktop -output target/universal/release/yt-desktop

echo "[*] Assembling macOS app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS" "$RESOURCES"

cp "$DIR/target/universal/release/yt-desktop" "$MACOS/yt-desktop"
cp "$DIR/tpws/tpws" "$MACOS/tpws"
chmod +x "$MACOS/yt-desktop" "$MACOS/tpws"

if [ -f "$DIR/assets/AppIcon.icns" ]; then
    cp "$DIR/assets/AppIcon.icns" "$RESOURCES/AppIcon.icns"
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
    <string>0.1.1</string>
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
