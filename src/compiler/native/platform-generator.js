// @ts-check
/**
 * Shared contract and helpers for Tac native host generators.
 *
 * A native host is the small native application (macOS .app, Windows .exe,
 * etc.) that loads the Tac static assets produced by the compiler into a
 * platform webview. It contains no embedded Yon backend; it is frontend-only.
 */

import { chmod, cp, mkdir, readFile, rm, writeFile } from 'fs/promises';
import path from 'path';
import NativeIconAssets from './icon-assets.js';
import { nativeHostCapabilities, nativeRawHostCapabilities, TAC_NATIVE_BRIDGE_ABI_VERSION } from './host-capabilities.js';

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
 */

/**
 * @abstract
 */
export default class PlatformGenerator {
    /** @type {string} */
    target;
    /** @type {string} */
    assetRoot;
    /** @type {string} */
    outputRoot;
    /** @type {string} */
    appName;
    /** @type {string} */
    appId;
    /** @type {string} */
    version;
    /** @type {string} */
    resourcesDir;
    /** @type {Array<{ route: string, capability: string, descriptor: unknown }>} */
    nativeCapabilities;
    /** @type {Set<string>} */
    requestedDevicePermissions;
    /** @type {Set<string>} */
    requestedNativeCapabilities;

    /**
     * @param {NativeHostOptions} options
     */
    constructor(options) {
        this.target = options.target;
        this.assetRoot = path.resolve(options.assetRoot);
        this.outputRoot = path.resolve(options.outputRoot);
        this.appName = options.appName || 'TachyonApp';
        const appIdSuffix = this.appName.replace(/[^a-zA-Z0-9]/g, '').toLowerCase() || 'app';
        this.appId = options.appId || `ma.del.tachyon.${appIdSuffix}`;
        this.version = options.version || '1.0.0';
        this.resourcesDir = path.join(this.outputRoot, 'Resources');
        this.nativeCapabilities = [];
        this.requestedDevicePermissions = new Set(options.devicePermissions ?? []);
        this.requestedNativeCapabilities = new Set(options.nativeCapabilities ?? []);
    }

    /**
     * Main entry point. Copies assets and delegates project-file generation.
     * @returns {Promise<void>}
     */
    async generate() {
        await this.prepareOutputDirectory();
        await this.copyAssets();
        await this.writeDevicePermissionMeta();
        await this.writeAppIcons();
        await this.writeHostManifest();
        await this.generateProjectFiles();
        await this.writeReadme();
    }

    /**
     * @returns {Promise<void>}
     */
    async prepareOutputDirectory() {
        await rm(this.outputRoot, { recursive: true, force: true });
        await mkdir(this.outputRoot, { recursive: true });
    }

    /**
     * Copies the static Tac assets into the host resource directory.
     * @returns {Promise<void>}
     */
    async copyAssets() {
        await mkdir(this.resourcesDir, { recursive: true });
        await cp(this.assetRoot, this.resourcesDir, {
            recursive: true,
            preserveTimestamps: true,
            filter: (source) => !path.basename(source).startsWith('.'),
        });
    }

    /** @returns {Promise<void>} */
    async writeAppIcons() {
        await new NativeIconAssets({
            outputRoot: this.outputRoot,
            resourcesDir: this.resourcesDir,
        }).write();
    }

    /** @returns {Promise<void>} */
    async writeDevicePermissionMeta() {
        const indexPath = path.join(this.resourcesDir, 'index.html');
        let html;
        try { html = await readFile(indexPath, 'utf8'); }
        catch { return; }
        const permissions = [...this.requestedDevicePermissions].sort().join(',');
        const meta = `    <meta name="tachyon-device-permissions" content="${permissions}">`;
        if (html.includes('name="tachyon-device-permissions"')) return;
        await writeFile(indexPath, html.replace('</head>', `${meta}\n</head>`));
    }

    /**
     * Writes a manifest describing the host and its runtime contract.
     * @returns {Promise<void>}
     */
    async writeHostManifest() {
        const capabilities = await this.collectNativeCapabilities();
        this.nativeCapabilities = capabilities;
        const manifest = {
            schemaVersion: 1,
            target: this.target,
            appName: this.appName,
            appId: this.appId,
            version: this.version,
            entry: 'Resources/index.html',
            // The public companion prelude is language-specific, but all calls
            // converge on this versioned message ABI at the host boundary.
            platformApiVersion: TAC_NATIVE_BRIDGE_ABI_VERSION,
            bridgeVersion: 2,
            hostCapabilities: nativeHostCapabilities(this.target, [...this.requestedNativeCapabilities]),
            rawHostCapabilities: nativeRawHostCapabilities(this.target, [...this.requestedNativeCapabilities]),
            requestedDevicePermissions: [...this.requestedDevicePermissions].sort(),
            capabilities,
        };
        const manifestPath = path.join(this.outputRoot, 'tachyon.host.json');
        await writeFile(manifestPath, JSON.stringify(manifest, null, 2));
    }

    /**
     * Records every operation concretely enabled for this host. Raw filesystem
     * and desktop shell operations require an explicit package declaration.
     * @returns {Promise<Array<{ route: string, capability: string, descriptor: unknown }>>}
     */
    async collectNativeCapabilities() {
        return [...nativeHostCapabilities(this.target, [...this.requestedNativeCapabilities])]
            .map((capability) => ({ route: '*', capability, descriptor: {} }))
            .sort((left, right) => left.capability.localeCompare(right.capability));
    }


    /**
     * Returns the inline bridge script that the native host injects into the
     * loaded page. Subclasses may override the exact injection mechanism, but
     * the global `window.__tcNativeBridge__` contract should stay consistent.
     * @returns {string}
     */
    getBridgeScript() {
        const hostCapabilities = nativeHostCapabilities(this.target, [...this.requestedNativeCapabilities]);
        return `(function () {
  const pending = new Map();
  const listeners = new Set();
  const queuedHostEvents = [];
  const hostCapabilities = new Set(${JSON.stringify(hostCapabilities)});
  let sequence = 0;

  function hasNativeHost() {
    return !!(
      (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.tachyon) ||
      window.__tcNativeHost__
    );
  }

  function postNative(message) {
    const payload = typeof message === 'string' ? message : JSON.stringify(message);
    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.tachyon) {
      window.webkit.messageHandlers.tachyon.postMessage(payload);
      return true;
    }
    if (window.__tcNativeHost__) {
      window.__tcNativeHost__.postMessage(payload);
      return true;
    }
    return false;
  }

  async function fallbackInvoke(capability, payload) {
    const data = payload && typeof payload === 'object' ? payload : {};
    if (capability === 'app.info') {
      return {
        name: document.title || 'Tachyon App',
        runtime: 'native-webview',
        href: location.href,
        userAgent: navigator.userAgent,
        language: navigator.language,
        online: navigator.onLine,
      };
    }
    if (capability === 'clipboard.readText' && navigator.clipboard?.readText) {
      return await navigator.clipboard.readText();
    }
    if (capability === 'clipboard.writeText' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(String(data.text ?? ''));
      return { written: true };
    }
    if (capability === 'openUrl') {
      const url = String(data.url ?? '');
      if (!/^https?:\\/\\//i.test(url)) throw new Error('openUrl requires an http(s) URL');
      window.open(url, '_blank', 'noopener,noreferrer');
      return { opened: true };
    }
    if (capability === 'share.text' && navigator.share) {
      await navigator.share({ text: String(data.text ?? ''), title: String(data.title ?? '') });
      return { shared: true };
    }
    if (capability === 'haptics.impact' && navigator.vibrate) {
      navigator.vibrate(10);
      return { impacted: true };
    }
    if (capability === 'file.openText' && typeof window.showOpenFilePicker === 'function') {
      const handles = await window.showOpenFilePicker({
        multiple: false,
        types: [{ description: 'Text files', accept: { 'text/*': ['.txt', '.md', '.json', '.csv'] } }],
      });
      const file = await handles[0].getFile();
      return { name: file.name, text: await file.text() };
    }
    throw new Error("Native capability '" + capability + "' is not available");
  }

  function invoke(capability, payload = {}, options = {}) {
    const fallbackFirst = ['app.info', 'clipboard.readText', 'clipboard.writeText', 'openUrl', 'file.openText'].includes(capability);
    if (!hasNativeHost() || (fallbackFirst && !hostCapabilities.has(capability))) return fallbackInvoke(capability, payload);
    const id = ++sequence;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        pending.delete(id);
        reject(new Error("Native capability '" + capability + "' timed out"));
      }, Number(options.timeoutMs || 10000));
      pending.set(id, { resolve, reject, timeout });
      if (!postNative({
        type: 'tac:native-request',
        id,
        capability,
        payload,
        source: String(options.source || ''),
      })) {
        clearTimeout(timeout);
        pending.delete(id);
        reject(new Error('Native host bridge is not available'));
      }
    });
  }

  function messageHandler(raw) {
    let message = raw;
    if (typeof raw === 'string') {
      try { message = JSON.parse(raw); } catch { message = { type: 'message', value: raw }; }
    }
    if (message && typeof message === 'object' && message.type === 'tac:native-response') {
      const pendingCall = pending.get(Number(message.id));
      if (pendingCall) {
        pending.delete(Number(message.id));
        clearTimeout(pendingCall.timeout);
        if (message.ok) pendingCall.resolve(message.value);
        else pendingCall.reject(new Error(String(message.error || 'Native capability failed')));
      }
      return;
    }
    if (message && typeof message === 'object' && message.type === 'tac:host-event' && listeners.size === 0) {
      queuedHostEvents.push(message);
      return;
    }
    for (const listener of listeners) listener(message);
  }

  window.__tcNativeBridge__ = {
    version: 2,
    postMessage: postNative,
    invoke,
    onMessage(handler) {
      listeners.add(handler);
      for (const event of queuedHostEvents.splice(0)) handler(event);
      return () => listeners.delete(handler);
    },
    messageHandler,
  };
  // An ordered, buffered lifecycle event proves the host-event contract even
  // before a platform adds deep links or notification actions. Native hosts
  // use this same envelope when they initiate events later in their lifecycle.
  setTimeout(() => messageHandler({ type: 'tac:host-event', event: 'app.ready', payload: { target: ${JSON.stringify(this.target)} } }), 0);
})();`;
    }

    /**
     * Escapes a string for safe inclusion in generated source files.
     * @param {string} value
     * @returns {string}
     */
    escapeString(value) {
        return value
            .replace(/\\/g, '\\\\')
            .replace(/"/g, '\\"')
            .replace(/\n/g, '\\n')
            .replace(/\r/g, '\\r');
    }

    /**
     * Creates a valid source-code identifier from the app name.
     * @param {string} fallback
     * @returns {string}
     */
    sourceIdentifier(fallback = 'TachyonApp') {
        const identifier = this.appName
            .split(/[^a-zA-Z0-9]+/)
            .filter(Boolean)
            .map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`)
            .join('');
        if (!identifier)
            return fallback;
        return /^[0-9]/.test(identifier) ? `${fallback}${identifier}` : identifier;
    }

    /**
     * Writes a human-readable README explaining how to build the host.
     * @returns {Promise<void>}
     */
    async writeReadme() {
        const text = this.buildReadme();
        if (!text) return;
        await writeFile(path.join(this.outputRoot, 'README.md'), text);
    }

    /**
     * Writes a script and marks it executable on POSIX file systems.
     * @param {string} relativePath
     * @param {string} contents
     * @returns {Promise<void>}
     */
    async writeExecutable(relativePath, contents) {
        const filePath = path.join(this.outputRoot, relativePath);
        await writeFile(filePath, contents);
        await chmod(filePath, 0o755).catch(() => { });
    }

    /**
     * @abstract
     * @returns {Promise<void>}
     */
    async generateProjectFiles() {
        throw new Error(`generateProjectFiles() must be implemented for target '${this.target}'`);
    }

    /**
     * @abstract
     * @returns {string | null}
     */
    buildReadme() {
        return null;
    }
}
