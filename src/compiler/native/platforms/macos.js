// @ts-check
import PlatformGenerator from '../platform-generator.js';
import NativeUIPlatformProject from '../../native-ui/platform-project.js';

/** Generates the native-first SwiftUI host shared by macOS and iOS. */
export default class MacOSGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        await NativeUIPlatformProject.generate(this);
    }

    infoPlist() {
        const usageDescriptions = [
            this.requestedDevicePermissions.has('camera')
                ? '    <key>NSCameraUsageDescription</key><string>Allow camera access for declared managed content origins.</string>'
                : '',
            this.requestedDevicePermissions.has('microphone')
                ? '    <key>NSMicrophoneUsageDescription</key><string>Allow microphone access for declared managed content origins.</string>'
                : '',
        ].filter(Boolean).join('\n');
        return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>${this.appName}</string>
    <key>CFBundleIdentifier</key><string>${this.appId}</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>${this.appName}</string>
    <key>CFBundleIconFile</key><string>TachyonIcon</string>
    <key>CFBundleDisplayName</key><string>${this.appName}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${this.version}</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
${usageDescriptions}
</dict></plist>
`;
    }

    entitlements() {
        const deviceEntitlements = [
            this.requestedDevicePermissions.has('camera')
                ? '    <key>com.apple.security.device.camera</key><true/>'
                : '',
            this.requestedDevicePermissions.has('microphone')
                ? '    <key>com.apple.security.device.audio-input</key><true/>'
                : '',
        ].filter(Boolean).join('\n');
        return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>com.apple.security.app-sandbox</key><true/>
    <key>com.apple.security.network.client</key><true/>
${deviceEntitlements}
</dict></plist>
`;
    }

    buildScript() {
        return `#!/bin/sh
set -eu
APP_NAME="${this.appName}"
APP_BUNDLE="$APP_NAME.app"
OUTPUT_ROOT="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$OUTPUT_ROOT/build"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/$APP_BUNDLE/Contents/MacOS" "$BUILD_DIR/$APP_BUNDLE/Contents/Resources"
cp "$OUTPUT_ROOT/TachyonApp/Info.plist" "$BUILD_DIR/$APP_BUNDLE/Contents/Info.plist"
cp "$OUTPUT_ROOT/TachyonApp/PkgInfo" "$BUILD_DIR/$APP_BUNDLE/Contents/PkgInfo"
cp "$OUTPUT_ROOT/TachyonApp/TachyonApp.entitlements" "$BUILD_DIR/$APP_BUNDLE/Contents/Resources/TachyonApp.entitlements"
cp -R "$OUTPUT_ROOT/Resources/"* "$BUILD_DIR/$APP_BUNDLE/Contents/Resources/"
swiftc \\
    -O \\
    -parse-as-library \\
    -target "$(uname -m)-apple-macos11" \\
    -framework Cocoa \\
    -framework WebKit \\
    "$OUTPUT_ROOT/Sources/TachyonApp.swift" \\
    -o "$BUILD_DIR/$APP_BUNDLE/Contents/MacOS/$APP_NAME"
if command -v xattr >/dev/null 2>&1; then xattr -cr "$BUILD_DIR/$APP_BUNDLE"; fi
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - --entitlements "$BUILD_DIR/$APP_BUNDLE/Contents/Resources/TachyonApp.entitlements" "$BUILD_DIR/$APP_BUNDLE"
fi
echo "Built: $BUILD_DIR/$APP_BUNDLE"
`;
    }
}
