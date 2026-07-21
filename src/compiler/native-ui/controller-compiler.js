// @ts-check
import { mkdir, mkdtemp, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { nativeUIRuntimeModuleSource } from '../../runtime/native-ui-runtime.js';

/** @param {unknown[]} logs */
function formatBuildLogs(logs) {
    return logs.map((log) => String(log)).join('\n');
}

export const NATIVE_UI_CONTROLLER_FILENAME = 'tachyon.native-controller.js';

/**
 * Square's Android QuickJS build evaluates classic scripts without BigInt
 * literal syntax. Keep strings and template payloads byte-for-byte while
 * lowering code literals to a constructor that uses native BigInt where the
 * host exposes it and Number otherwise. Native UI state itself is JSON, so a
 * BigInt cannot cross the controller boundary.
 *
 * @param {string} source
 */
function lowerClassicScriptBigInts(source) {
    let output = '';
    let state = 'code';
    let changed = false;
    const bigintLiteral = /(?:0[xX][0-9a-fA-F_]+|0[bB][01_]+|0[oO][0-7_]+|[0-9][0-9_]*)n\b/y;
    for (let index = 0; index < source.length;) {
        const character = source[index];
        const next = source[index + 1];
        if (state === 'line-comment') {
            output += character; index += 1;
            if (character === '\n') state = 'code';
            continue;
        }
        if (state === 'block-comment') {
            output += character; index += 1;
            if (character === '*' && next === '/') { output += next; index += 1; state = 'code'; }
            continue;
        }
        if (state === 'single' || state === 'double' || state === 'template') {
            output += character; index += 1;
            if (character === '\\' && index < source.length) { output += source[index]; index += 1; continue; }
            if ((state === 'single' && character === "'") || (state === 'double' && character === '"') || (state === 'template' && character === '`'))
                state = 'code';
            continue;
        }
        if (character === '/' && next === '/') { output += '//'; index += 2; state = 'line-comment'; continue; }
        if (character === '/' && next === '*') { output += '/*'; index += 2; state = 'block-comment'; continue; }
        if (character === "'") { output += character; index += 1; state = 'single'; continue; }
        if (character === '"') { output += character; index += 1; state = 'double'; continue; }
        if (character === '`') { output += character; index += 1; state = 'template'; continue; }
        if (character >= '0' && character <= '9') {
            bigintLiteral.lastIndex = index;
            const match = bigintLiteral.exec(source);
            if (match) {
                output += `__tc_bigint__(${JSON.stringify(match[0].slice(0, -1).replaceAll('_', ''))})`;
                index += match[0].length;
                changed = true;
                continue;
            }
        }
        output += character;
        index += 1;
    }
    return changed
        ? `var __tc_bigint__ = typeof BigInt === "function" ? BigInt : Number;\n${output}`
        : output;
}

/**
 * Bundles Tac's server-compiled render closures into a single, DOM-free
 * controller program. Native hosts evaluate this program in their embedded
 * JavaScript runtime and exchange versioned JSON snapshots and events with it.
 */
export default class NativeUIControllerCompiler {
    /**
     * @param {{
     *   routes: Array<{ route: string, modulePath: string }>,
     *   outputFile: string,
     *   adapters?: string[] | Record<string, string>,
     * }} options
     */
    static async compile(options) {
        if (!options.routes.length)
            throw new Error('A native UI controller requires at least one compiled route module.');

        const routeNames = new Set();
        for (const entry of options.routes) {
            if (routeNames.has(entry.route))
                throw new Error(`Native UI controller route '${entry.route}' is duplicated.`);
            routeNames.add(entry.route);
        }

        const temporaryRoot = await mkdtemp(path.join(tmpdir(), 'tachyon-native-ui-controller-'));
        try {
            const nativeUIRuntimeSource = nativeUIRuntimeModuleSource();
            const entryFile = path.join(temporaryRoot, 'entry.js');
            const imports = options.routes.map((entry, index) =>
                `import routeFactory${index} from ${JSON.stringify(path.resolve(entry.modulePath))};`,
            );
            const factories = options.routes.map((entry, index) =>
                `[${JSON.stringify(entry.route)}, routeFactory${index}]`,
            );
            const entryRoute = routeNames.has('/') ? '/' : options.routes[0].route;
            const source = `
import NativeUIRuntime from "tachyon:embedded/native-ui-runtime";
${imports.join('\n')}

function callNativeHost(capability, payload = {}) {
    if (typeof globalThis.__tachyonNativeHostCall !== "function")
        throw new Error("Native host bridge is unavailable");
    const raw = globalThis.__tachyonNativeHostCall(String(capability), JSON.stringify(payload ?? {}));
    const value = typeof raw === "string" ? JSON.parse(raw) : raw;
    if (!value || typeof value !== "object" || value.ok !== true) {
        throw new Error(String(value?.error || "Native host capability failed"));
    }
    return value.value;
}

let hostInfo = { target: "web", platform: "web", capabilities: [] };
if (typeof globalThis.__tachyonNativeHostCall === "function") {
    try { hostInfo = callNativeHost("__tachyon.hostInfo"); } catch {}
}
const hostCapabilities = new Set(Array.isArray(hostInfo.capabilities) ? hostInfo.capabilities : []);
const hostListeners = new Set();
globalThis.__tcNativeBridge__ = Object.freeze({
    version: 2,
    supports(capability) { return hostCapabilities.has(String(capability)); },
    async invoke(capability, payload = {}) { return callNativeHost(capability, payload); },
    onMessage(listener) {
        if (typeof listener !== "function") throw new TypeError("Native host listener must be a function.");
        hostListeners.add(listener);
        return () => hostListeners.delete(listener);
    },
});
globalThis.Tac = Object.assign(globalThis.Tac || {}, {
    platform: Object.freeze({
        target: hostInfo.target || "web",
        platform: hostInfo.platform || "web",
        environment: hostInfo.target || "web",
        os: hostInfo.target || "web",
        native: hostInfo.target !== "web",
        web: hostInfo.target === "web",
        desktop: hostInfo.platform === "desktop",
        mobile: hostInfo.platform === "mobile",
    }),
});

const routeFactories = new Map([${factories.join(',\n')}]);
const routeRuntimes = new Map();
const adapters = ${JSON.stringify(options.adapters ?? [])};
let activeRoute = ${JSON.stringify(entryRoute)};

async function getRuntime(route) {
    const factory = routeFactories.get(route);
    if (!factory) throw new Error("Native UI route '" + route + "' is not bundled.");
    if (!routeRuntimes.has(route)) {
        const renderClosure = await factory();
        if (typeof renderClosure !== 'function')
            throw new Error("Native UI route '" + route + "' did not create a render closure.");
        routeRuntimes.set(route, new NativeUIRuntime(renderClosure, { route, adapters }));
    }
    return routeRuntimes.get(route);
}

function decodeEvent(payload) {
    const event = typeof payload === 'string' ? JSON.parse(payload) : payload;
    if (!event || typeof event !== 'object')
        throw new Error('Native UI event payload must be a JSON object.');
    if (typeof event.elementId !== 'string' || !event.elementId)
        throw new Error('Native UI event payload requires elementId.');
    if (typeof event.type !== 'string' || !event.type)
        throw new Error('Native UI event payload requires type.');
    return event;
}

globalThis.__tachyonNativeUI = Object.freeze({
    schemaVersion: 1,
    routes: Object.freeze([...routeFactories.keys()]),
    async open(route) {
        activeRoute = String(route);
        return JSON.stringify(await (await getRuntime(activeRoute)).render());
    },
    async render() {
        return JSON.stringify(await (await getRuntime(activeRoute)).render());
    },
    async dispatch(payload) {
        return JSON.stringify(await (await getRuntime(activeRoute)).dispatch(decodeEvent(payload)));
    },
    async emit(payload) {
        const message = typeof payload === "string" ? JSON.parse(payload) : payload;
        if (!message || message.type !== "tac:host-event" || typeof message.event !== "string")
            throw new Error("Native host event payload is invalid.");
        for (const listener of [...hostListeners]) {
            try { listener(message); }
            catch (error) {
                if (typeof console !== "undefined" && typeof console.error === "function")
                    console.error("Native host event listener failed:", error);
            }
        }
        await Promise.resolve();
        return JSON.stringify(await (await getRuntime(activeRoute)).render());
    },
});
`;
            await writeFile(entryFile, source);
            const result = await Bun.build({
                entrypoints: [entryFile],
                target: 'browser',
                format: 'iife',
                splitting: false,
                minify: false,
                sourcemap: 'none',
                define: {
                    'import.meta.url': JSON.stringify('file:///tachyon.native-controller.js'),
                },
                plugins: [{
                    name: 'tachyon-native-ui-rooted-modules',
                    setup(build) {
                        build.onResolve({ filter: /^tachyon:embedded\/native-ui-runtime$/ }, () => ({
                            path: 'native-ui-runtime.js',
                            namespace: 'tachyon-native-ui-embedded',
                        }));
                        build.onLoad({ filter: /^native-ui-runtime\.js$/, namespace: 'tachyon-native-ui-embedded' }, () => ({
                            contents: nativeUIRuntimeSource,
                            loader: 'js',
                        }));
                        build.onResolve({ filter: /^\/(?:components|pages|shared)\// }, (args) => ({
                            path: path.join(path.dirname(options.outputFile), args.path.slice(1)),
                        }));
                    },
                }],
            });
            if (!result.success || !result.outputs[0])
                throw new Error(`Failed to bundle native UI controller.\n${formatBuildLogs(result.logs)}`.trim());
            await mkdir(path.dirname(options.outputFile), { recursive: true });
            await writeFile(options.outputFile, lowerClassicScriptBigInts(await result.outputs[0].text()));
            return options.outputFile;
        }
        finally {
            await rm(temporaryRoot, { recursive: true, force: true });
        }
    }
}
