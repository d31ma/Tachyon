// @ts-check
import PlatformGenerator from '../platform-generator.js';
import NativeUIPlatformProject from '../../native-ui/platform-project.js';

/** Generates the native-first GTK host for Linux. */
export default class LinuxGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        await NativeUIPlatformProject.generate(this);
    }
}
