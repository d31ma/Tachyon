// @ts-check
import PlatformGenerator from '../platform-generator.js';
import NativeUIPlatformProject from '../../native-ui/platform-project.js';

/** Generates the native-first WinUI host for Windows. */
export default class WindowsGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        await NativeUIPlatformProject.generate(this);
    }
}
