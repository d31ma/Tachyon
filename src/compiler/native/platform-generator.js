// @ts-check
/**
 * Shared contract and helpers for Tac native host generators.
 *
 * A native host is the small native application (macOS .app, Windows .exe,
 * etc.) that loads the Tac static assets produced by the compiler into a
 * platform webview. It contains no embedded Yon backend; it is frontend-only.
 */

import { chmod, cp, mkdir, readFile, readdir, rm, writeFile } from 'fs/promises';
import path from 'path';
import NativeIconAssets from './icon-assets.js';

/**
 * @typedef {object} NativeHostOptions
 * @property {string} target
 * @property {string} assetRoot
 * @property {string} outputRoot
 * @property {string} appName
 * @property {string} [appId]
 * @property {string} [version]
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
    }

    /**
     * Main entry point. Copies assets and delegates project-file generation.
     * @returns {Promise<void>}
     */
    async generate() {
        await this.prepareOutputDirectory();
        await this.copyAssets();
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
            bridgeVersion: 1,
            capabilities,
        };
        const manifestPath = path.join(this.outputRoot, 'tachyon.host.json');
        await writeFile(manifestPath, JSON.stringify(manifest, null, 2));
    }

    hasRawOsCapabilities() {
        const authorized = new Set(
            (process.env.TAC_DANGEROUS_CAPABILITIES || '')
                .split(',')
                .map((capability) => capability.trim())
                .filter(Boolean),
        );
        return this.nativeCapabilities.some(({ capability }) => {
            const raw = /^(fs\.|shell\.|process\.)/.test(capability);
            return raw && authorized.has(capability);
        });
    }

    /**
     * Reads the build-wide `TAC_NATIVE_CAPABILITIES` environment variable and
     * records authorized native capabilities for host packaging/auditing. The
     * runtime enforces the allowlist inside each worker; this manifest is for
     * native-shell visibility.
     * @returns {Promise<Array<{ route: string, capability: string, descriptor: unknown }>>}
     */
    async collectNativeCapabilities() {
        const caps = (process.env.TAC_NATIVE_CAPABILITIES || '')
            .split(',')
            .map((capability) => capability.trim())
            .filter(Boolean);
        return caps
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
        return `(function () {
  const pending = new Map();
  const listeners = new Set();
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
    if (fallbackFirst || !hasNativeHost()) return fallbackInvoke(capability, payload);
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
    for (const listener of listeners) listener(message);
  }

  window.__tcNativeBridge__ = {
    version: 2,
    postMessage: postNative,
    invoke,
    onMessage(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
    messageHandler,
  };
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
