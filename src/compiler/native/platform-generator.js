// @ts-check
/** Shared lifecycle for native-first host project generators. */

import { access, chmod, cp, mkdir, readFile, rm, writeFile } from 'fs/promises';
import path from 'path';
import NativeIconAssets from './icon-assets.js';
import { TAC_NATIVE_BRIDGE_ABI_VERSION } from './host-capabilities.js';

/**
 * @typedef {object} NativeHostOptions
 * @property {string} target
 * @property {string} assetRoot
 * @property {string} outputRoot
 * @property {string} appName
 * @property {string} [appId]
 * @property {string} [version]
 * @property {string[]} [devicePermissions]
 * @property {string[]} [nativeCapabilities]
 * @property {Record<string, string[]>} [permissionOrigins]
 * @property {string[]} [managedContentOrigins]
 * @property {unknown[]} [nativeHostExtensions]
 * @property {unknown} [nativeUIAdapters]
 */

/** @abstract */
export default class PlatformGenerator {
    /** @param {NativeHostOptions} options */
    constructor(options) {
        this.target = options.target;
        this.assetRoot = path.resolve(options.assetRoot);
        this.outputRoot = path.resolve(options.outputRoot);
        this.appName = options.appName || 'TachyonApp';
        const suffix = this.appName.replace(/[^a-zA-Z0-9]/g, '').toLowerCase() || 'app';
        this.appId = options.appId || `ma.del.tachyon.${suffix}`;
        this.version = options.version || '1.0.0';
        this.resourcesDir = path.join(this.outputRoot, 'Resources');
        this.requestedDevicePermissions = new Set(options.devicePermissions ?? []);
        this.requestedNativeCapabilities = new Set(options.nativeCapabilities ?? []);
        this.permissionOrigins = options.permissionOrigins ?? {};
        this.managedContentOrigins = options.managedContentOrigins ?? [];
        this.nativeHostExtensionSpecifiers = options.nativeHostExtensions ?? [];
        this.nativeUIAdapters = options.nativeUIAdapters ?? [];
        this.renderMode = /** @type {const} */ ('native');
        this.hasWebViewFallbacks = false;
    }

    async generate() {
        await this.validateRenderAssets();
        await this.prepareOutputDirectory();
        await this.copyAssets();
        await new NativeIconAssets({ outputRoot: this.outputRoot, resourcesDir: this.resourcesDir }).write();
        await this.writeHostManifest();
        await this.generateProjectFiles();
        await this.writeReadme();
    }

    /** Validates the native tree and rejects removed compatibility-host APIs. */
    async validateRenderAssets() {
        const compatibilityOnly = [];
        if (this.nativeHostExtensionSpecifiers.length) compatibilityOnly.push('nativeHostExtensions');
        if (this.requestedNativeCapabilities.size) compatibilityOnly.push('nativeCapabilities');
        if (this.requestedDevicePermissions.size) compatibilityOnly.push('devicePermissions');
        if (Object.keys(this.permissionOrigins).length) compatibilityOnly.push('permissionOrigins');
        if (this.managedContentOrigins.length) compatibilityOnly.push('managedContentOrigins');
        if (compatibilityOnly.length) {
            throw new Error(`Native-first bundles do not expose ${compatibilityOnly.join(', ')} yet. Add native-tree capability adapters before requesting them.`);
        }

        const documentPath = path.join(this.assetRoot, 'tachyon.native-ui.json');
        let bundle;
        try {
            bundle = JSON.parse(await readFile(documentPath, 'utf8'));
        } catch (error) {
            throw new Error(`Native render mode requires a valid tachyon.native-ui.json in ${this.assetRoot}: ${error}`);
        }
        if (bundle?.schemaVersion !== 1 || bundle?.renderMode !== 'native') {
            throw new Error('Native UI bundle must use schemaVersion 1 and the native-first renderer.');
        }
        this.hasWebViewFallbacks = bundle.hasWebViewFallbacks === true
            || (Array.isArray(bundle.webViewFallbacks) && bundle.webViewFallbacks.length > 0);
        if (typeof bundle.controller !== 'string' || !bundle.controller) {
            throw new Error('Native UI bundle is missing its controller entry.');
        }
        await access(path.join(this.assetRoot, bundle.controller)).catch(() => {
            throw new Error(`Native UI controller '${bundle.controller}' is missing from ${this.assetRoot}.`);
        });
    }

    async prepareOutputDirectory() {
        await rm(this.outputRoot, { recursive: true, force: true });
        await mkdir(this.outputRoot, { recursive: true });
    }

    async copyAssets() {
        await mkdir(this.resourcesDir, { recursive: true });
        await cp(this.assetRoot, this.resourcesDir, {
            recursive: true,
            preserveTimestamps: true,
            filter: (source) => !path.basename(source).startsWith('.'),
        });
    }

    async writeHostManifest() {
        const resourceRoot = path.relative(this.outputRoot, this.resourcesDir).replaceAll(path.sep, '/');
        const nativeUIEntry = `${resourceRoot}/tachyon.native-ui.json`;
        const manifest = {
            schemaVersion: 2,
            target: this.target,
            appName: this.appName,
            appId: this.appId,
            version: this.version,
            entry: nativeUIEntry,
            renderMode: 'native',
            nativeUIEntry,
            hasWebViewFallbacks: this.hasWebViewFallbacks,
            platformApiVersion: TAC_NATIVE_BRIDGE_ABI_VERSION,
            bridgeVersion: 2,
            hostCapabilities: [],
            rawHostCapabilities: [],
            requestedDevicePermissions: [],
            permissionOrigins: {},
            managedContentPolicy: {
                allowedOrigins: [],
                layout: { mode: 'split', edge: 'right', ratio: 0.75 },
                popups: 'event',
                downloads: 'deny',
                uploads: 'prompt',
                permissions: 'deny-all',
            },
            extensions: [],
            capabilities: [],
        };
        await writeFile(path.join(this.outputRoot, 'tachyon.host.json'), JSON.stringify(manifest, null, 2));
    }

    async writeReadme() {
        const boundary = this.hasWebViewFallbacks
            ? ' Unmapped HTML and Web Component subtrees use isolated WebView boundaries while native siblings remain native.'
            : ' This bundle does not require a WebView boundary.';
        await writeFile(path.join(this.outputRoot, 'README.md'), `# ${this.appName} — ${this.target} native UI host

This project renders Tac's strict HTML through the platform UI toolkit.${boundary}
The authored HTML is lowered into \`${path.basename(this.resourcesDir)}/tachyon.native-ui.json\`;
live state and events run through \`tachyon.native-controller.js\`.

See \`tachyon.host.json\` for the versioned runtime contract.
`);
    }

    /** @param {string} relativePath @param {string} contents */
    async writeExecutable(relativePath, contents) {
        const filePath = path.join(this.outputRoot, relativePath);
        await writeFile(filePath, contents);
        await chmod(filePath, 0o755).catch(() => {});
    }

    /** @abstract */
    async generateProjectFiles() {
        throw new Error(`generateProjectFiles() must be implemented for target '${this.target}'`);
    }
}
