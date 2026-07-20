// @ts-check
import path from 'path';
import PlatformGenerator from '../platform-generator.js';
import NativeUIPlatformProject from '../../native-ui/platform-project.js';

/** Generates the native-first SwiftUI host for iOS. */
export default class IOSGenerator extends PlatformGenerator {
    /** @param {ConstructorParameters<typeof PlatformGenerator>[0]} options */
    constructor(options) {
        super(options);
        // Avoid the top-level Resources name, which iOS treats as a shallow
        // macOS bundle during signing and installation.
        this.resourcesDir = path.join(this.outputRoot, 'WebBundle');
    }

    async generateProjectFiles() {
        await NativeUIPlatformProject.generate(this);
    }

    xcodegenSpec() {
        return `name: ${this.appName}
options:
  createIntermediateGroups: true
targets:
  ${this.appName}:
    type: application
    platform: iOS
    deploymentTarget: "15.0"
    sources:
      - path: Sources
      - path: Assets.xcassets
      - path: WebBundle
        type: folder
    info:
      path: Info.plist
      properties:
        CFBundleDisplayName: ${this.appName}
        CFBundleShortVersionString: "${this.version}"
        CFBundleVersion: "1"
        UILaunchScreen: {}
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: ${this.appId}
        TARGETED_DEVICE_FAMILY: "1,2"
        SWIFT_VERSION: "5.9"
        ASSETCATALOG_COMPILER_APPICON_NAME: AppIcon
`;
    }
}
