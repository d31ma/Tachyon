// @ts-check
import Router from "../server/http/route-handler.js";
import { createPublicBrowserEnvResponse, PUBLIC_BROWSER_ENV_PATH, withPublicBrowserEnv } from "../server/http/browser-env.js";
import { isNativeTarget, targetContext } from "../shared/native-targets.js";
import logger from "../server/observability/logger.js";
import { existsSync } from 'fs';
import { access, mkdir, mkdtemp, readdir, readFile, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from "path";
import { pathToFileURL } from "url";
import { EVENT_CAPTURE_SCRIPT } from "../runtime/event-capture-script.js";
import TachyonRuntimeCache from "../shared/runtime-cache.js";
import DartCompanionCompiler from './dart-companion.js';
import TacSubsetCompanionCompiler from './subset-companion.js';
import { nativeHostCapabilities } from './native/host-capabilities.js';
/**
 * @typedef {import("bun").BunRequest} BunRequest
 * @typedef {import("bun").BunPlugin} BunPlugin
 * @typedef {{ path: string, allowSelf: boolean }} WrapperPageEntry
 * @typedef {Record<string, WrapperPageEntry>} WrapperPageMap
 * @typedef {'esm' | 'global'} OutputFormat
 * @typedef {'ecmascript' | 'dart' | 'subset'} CompanionTarget
 * @typedef {'rust' | 'kotlin' | 'swift' | 'csharp'} TacSubsetLanguage
 * @typedef {{ extension: string, language: string, target: CompanionTarget, portable: true }} CompanionProvider
 * @typedef {{ sourcePath: string, importPath: string, provider: CompanionProvider }} CompanionScript
 * @typedef {{ html: string, hasSlot: boolean, script?: string, scriptLang: string, companion?: CompanionScript, companionImportPath?: string }} TemplateData
 * @typedef {{ static?: string, element?: string }} TemplateNode
 * @typedef {Record<string, string>} AttributeMap
 * @typedef {{ target: string, platform: string, environment: string, os: string, native: boolean, browser: boolean, web: boolean, desktop: boolean, mobile: boolean }} TacPlatformContext
 * @typedef {{ version: string, modules: Map<string, Function>, platform: TacPlatformContext, register: (id: string, factory: Function) => Function, load: (id: string) => Promise<Function> }} TacRegistry
 */
const ROUTE_MANIFEST_PLACEHOLDER = '{"__tachyonPlaceholder":true}';
const WRAPPER_MANIFEST_PLACEHOLDER = '{"__tachyonShellPlaceholder":true}';
const PRERENDER_WORKER_PATH = `${import.meta.dir}/../runtime/prerender-worker.js`;
const MODULES_ROUTE_PREFIX = '/shared/modules';
const IMPORT_MAP_PLACEHOLDER = '<!--__TACHYON_IMPORT_MAP__-->';
const FRAMEWORK_SOURCE_ROOT = path.resolve(import.meta.dir, '..');
const EVENT_CAPTURE_SCRIPT_SOURCE = `// @ts-check\nexport const EVENT_CAPTURE_SCRIPT = ${JSON.stringify(EVENT_CAPTURE_SCRIPT)};\n`;
/** @type {Promise<string> | null} */
let embeddedFrameworkRootPromise = null;
const EMBEDDED_FRAMEWORK_FILES = [
    'compiler/render-template.js',
    'runtime/shells/app.html',
    'runtime/shells/not-found.html',
    'runtime/spa-renderer.js',
    'runtime/dom-helpers.js',
    'runtime/static-cache.js',
    'runtime/event-capture-script.js',
    'runtime/event-hydration.js',
    'runtime/component-registry.js',
    'runtime/service-worker-policy.js',
    'runtime/fylo-browser-worker.js',
    'runtime/hot-reload-client.js',
    'runtime/tachyon-sw.js',
    'runtime/decorators.js',
    'runtime/fylo-global.js',
    'runtime/fylo-browser-sync.js',
    'runtime/browser-cache.js',
    'vendor/fylo/fylo-web.mjs',
];
function isEmbeddedFrameworkRuntime() {
    return import.meta.dir.startsWith('/$bunfs/');
}
/** @param {Promise<unknown>} modulePromise */
async function textModule(modulePromise) {
    return /** @type {{ default: string }} */ (await modulePromise).default;
}
/** @param {string} relativePath */
async function loadEmbeddedFrameworkSource(relativePath) {
    switch (relativePath) {
        case 'compiler/render-template.js': return textModule(import('./render-template.js', { with: { type: 'text' } }));
        case 'runtime/shells/app.html': return textModule(import('../runtime/shells/app.html', { with: { type: 'text' } }));
        case 'runtime/shells/not-found.html': return textModule(import('../runtime/shells/not-found.html', { with: { type: 'text' } }));
        case 'runtime/spa-renderer.js': return textModule(import('../runtime/spa-renderer.js', { with: { type: 'text' } }));
        case 'runtime/dom-helpers.js': return textModule(import('../runtime/dom-helpers.js', { with: { type: 'text' } }));
        case 'runtime/static-cache.js': return textModule(import('../runtime/static-cache.js', { with: { type: 'text' } }));
        case 'runtime/event-capture-script.js': return EVENT_CAPTURE_SCRIPT_SOURCE;
        case 'runtime/event-hydration.js': return textModule(import('../runtime/event-hydration.js', { with: { type: 'text' } }));
        case 'runtime/component-registry.js': return textModule(import('../runtime/component-registry.js', { with: { type: 'text' } }));
        case 'runtime/service-worker-policy.js': return textModule(import('../runtime/service-worker-policy.js', { with: { type: 'text' } }));
        case 'runtime/fylo-browser-worker.js': return textModule(import('../runtime/fylo-browser-worker.js', { with: { type: 'text' } }));
        case 'runtime/hot-reload-client.js': return textModule(import('../runtime/hot-reload-client.js', { with: { type: 'text' } }));
        case 'runtime/tachyon-sw.js': return textModule(import('../runtime/tachyon-sw.js', { with: { type: 'text' } }));
        case 'runtime/decorators.js': return textModule(import('../runtime/decorators.js', { with: { type: 'text' } }));
        case 'runtime/fylo-global.js': return textModule(import('../runtime/fylo-global.js', { with: { type: 'text' } }));
        case 'runtime/fylo-browser-sync.js': return textModule(import('../runtime/fylo-browser-sync.js', { with: { type: 'text' } }));
        case 'runtime/browser-cache.js': return textModule(import('../runtime/browser-cache.js', { with: { type: 'text' } }));
        case 'vendor/fylo/fylo-web.mjs': return textModule(import('../vendor/fylo/fylo-web.mjs', { with: { type: 'text' } }));
        default: throw new Error(`Unknown embedded Tachyon framework file: ${relativePath}`);
    }
}
async function embeddedFrameworkRoot() {
    if (!isEmbeddedFrameworkRuntime())
        return FRAMEWORK_SOURCE_ROOT;
    embeddedFrameworkRootPromise ??= (async () => {
        const files = await Promise.all(EMBEDDED_FRAMEWORK_FILES.map(async (relativePath) => ({
            path: relativePath,
            source: await loadEmbeddedFrameworkSource(relativePath),
        })));
        return TachyonRuntimeCache.materialize(files);
    })();
    return embeddedFrameworkRootPromise;
}
/** @param {string} relativePath */
async function frameworkFilePath(relativePath) {
    return path.join(await embeddedFrameworkRoot(), relativePath);
}
/**
 * @param {string} relativePath
 */
async function frameworkText(relativePath) {
    if (isEmbeddedFrameworkRuntime())
        return loadEmbeddedFrameworkSource(relativePath);
    return Bun.file(path.join(FRAMEWORK_SOURCE_ROOT, relativePath)).text();
}
/**
 * @param {string} routePath
 * @param {BodyInit} body
 * @param {string} [contentType]
 * @returns {Response}
 */
const staticRouteResponse = (routePath, body, contentType) => {
    const headers = new Headers();
    if (contentType)
        headers.set('Content-Type', contentType);
    headers.set('Cache-Control', Router.getCacheControlHeader(routePath, contentType));
    return new Response(body, { headers });
};
/** @param {string} routePath @param {BodyInit} body */
const jsResponse = (routePath, body) => staticRouteResponse(routePath, body, 'application/javascript');
/** @param {string} routePath @param {BodyInit} body @param {string} [contentType] */
const typedResponse = (routePath, body, contentType) => staticRouteResponse(routePath, body, contentType);
/** @param {string} filePath */
const pathExists = async (filePath) => {
    try {
        await access(filePath);
        return true;
    }
    catch {
        return false;
    }
};
export default class Compiler {
    static compilerLogger = logger.child({ scope: 'compiler' });
    static pageFileName = Router.pageFileName;
    static prerenderRenderConcurrency = 4;
    static prerenderWriteConcurrency = 8;
    /** @type {{ hits: number, misses: number }} */
    static prerenderCacheStats = { hits: 0, misses: 0 };
    static compileConcurrency = 8;
    /** @type {OutputFormat} */
    static outputFormat = Compiler.resolveOutputFormat();
    /** @type {string[]} */
    static mainEntryCandidates = ['imports.ts', 'imports.js'];
    /** @type {CompanionProvider[]} */
    static companionProviders = [
        { extension: '.js', language: 'javascript', target: 'ecmascript', portable: true },
        { extension: '.ts', language: 'typescript', target: 'ecmascript', portable: true },
        { extension: '.dart', language: 'dart', target: 'dart', portable: true },
        { extension: '.rs', language: 'rust', target: 'subset', portable: true },
        { extension: '.kt', language: 'kotlin', target: 'subset', portable: true },
        { extension: '.swift', language: 'swift', target: 'subset', portable: true },
        { extension: '.cs', language: 'csharp', target: 'subset', portable: true },
    ];
    /** @type {string[]} */
    static companionScriptExtensions = Compiler.companionProviders.map((provider) => provider.extension);
    /** Names of decorators auto-imported into companion scripts when referenced as `@<name>`. */
    static companionDecoratorNames = ['subscribe', 'publish', 'env', 'onMount'];
    /** Decorators intentionally removed from the strict v2 Tac surface. */
    static removedCompanionDecoratorNames = ['render'];
    /** @type {Set<string>} */
    static browserRuntimeRoutes = new Set();
    /** @type {Map<string, string>} */
    static compMapping = new Map();
    /** @type {WrapperPageMap} */
    static wrapperPages = {};
    /** @type {Set<string>} */
    static warnedUnknownTags = new Set();
    static warnedBareHandlers = new Set();
    // Native GlobalEventHandler content attributes. A bare one on a Tac template
    // (`onclick`) is almost always a typo for the `on:<event>` directive. Matched
    // by exact name — not `/^on\w+$/` — so real attributes like `onboarding`,
    // `online`, `onsale` don't trip the warning.
    // ponytail: common subset; extend if authors mistype a handler not listed here.
    static nativeEventHandlerAttrs = new Set([
        'onabort', 'onblur', 'oncancel', 'oncanplay', 'onchange', 'onclick', 'onclose',
        'oncontextmenu', 'oncopy', 'oncut', 'ondblclick', 'ondrag', 'ondragend',
        'ondragenter', 'ondragleave', 'ondragover', 'ondragstart', 'ondrop', 'onerror',
        'onfocus', 'oninput', 'oninvalid', 'onkeydown', 'onkeypress', 'onkeyup', 'onload',
        'onmousedown', 'onmouseenter', 'onmouseleave', 'onmousemove', 'onmouseout',
        'onmouseover', 'onmouseup', 'onpaste', 'onpointercancel', 'onpointerdown',
        'onpointerenter', 'onpointerleave', 'onpointermove', 'onpointerout',
        'onpointerover', 'onpointerup', 'onreset', 'onscroll', 'onselect', 'onsubmit',
        'ontoggle', 'ontouchend', 'ontouchmove', 'ontouchstart', 'onwheel',
    ]);
    /** @type {Set<string>} */
    static bundledDependencyModules = new Set();
    static nativeTags = new Set([
        'a', 'abbr', 'address', 'area', 'article', 'aside', 'audio', 'b', 'base', 'bdi', 'bdo',
        'blockquote', 'body', 'br', 'button', 'canvas', 'caption', 'cite', 'code', 'col',
        'colgroup', 'data', 'datalist', 'dd', 'del', 'details', 'dfn', 'dialog', 'div', 'dl',
        'dt', 'em', 'embed', 'fieldset', 'figcaption', 'figure', 'footer', 'form', 'h1', 'h2',
        'h3', 'h4', 'h5', 'h6', 'head', 'header', 'hgroup', 'hr', 'html', 'i', 'iframe', 'img',
        'input', 'ins', 'kbd', 'label', 'legend', 'li', 'link', 'main', 'map', 'mark', 'menu',
        'meta', 'meter', 'nav', 'noscript', 'object', 'ol', 'optgroup', 'option', 'output', 'p',
        'picture', 'pre', 'progress', 'q', 'rp', 'rt', 'ruby', 's', 'samp', 'script', 'search',
        'section', 'select', 'slot', 'small', 'source', 'span', 'strong', 'style', 'sub',
        'summary', 'sup', 'table', 'tbody', 'td', 'template', 'textarea', 'tfoot', 'th', 'thead',
        'time', 'title', 'tr', 'track', 'u', 'ul', 'var', 'video', 'wbr',
        'svg', 'g', 'path', 'circle', 'ellipse', 'line', 'polygon', 'polyline', 'rect', 'defs',
        'lineargradient', 'radialgradient', 'stop', 'symbol', 'use', 'clippath', 'mask', 'text',
        'tspan', 'foreignobject', 'animate', 'animatemotion', 'animatetransform', 'pattern',
        'marker', 'filter', 'feblend', 'fecolormatrix', 'fecomponenttransfer', 'fecomposite',
        'feconvolvematrix', 'fediffuselighting', 'fedisplacementmap', 'fedistantlight',
        'fedropshadow', 'feflood', 'fefunca', 'fefuncb', 'fefuncg', 'fefuncr', 'fegaussianblur',
        'feimage', 'femerge', 'femergenode', 'femorphology', 'feoffset', 'fepointlight',
        'fespecularlighting', 'fespotlight', 'fetile', 'feturbulence'
    ]);
    static controlTags = new Set(['loop', 'logic', 'switch', 'case']);
    static reservedCustomElementNames = new Set([
        'annotation-xml',
        'color-profile',
        'font-face',
        'font-face-src',
        'font-face-uri',
        'font-face-format',
        'font-face-name',
        'missing-glyph',
    ]);
    static routeTitleFallback = "Tachyon";
    /** @param {string} value */
    static escapeRegExp(value) {
        return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    }

    /**
     * Deterministic 8-char base36 hash of a string (FNV-1a). Used for stable
     * element ids and prerender cache keys — same input always maps to the same
     * id, so unchanged source reproduces byte-identical build output.
     * @param {string} value
     */
    static deterministicHash(value) {
        let hash = 2166136261;
        for (let index = 0; index < value.length; index++) {
            hash ^= value.charCodeAt(index);
            hash = Math.imul(hash, 16777619);
        }
        return (hash >>> 0).toString(36).padStart(8, '0').slice(-8);
    }

    /**
     * Per-compile element/host id generator. Deterministic by default: a stable
     * prefix derived from `seed` (the module's source identity) plus a monotonic
     * counter. Ids are therefore unique within the module *by construction* (the
     * counter never repeats — no hash-collision risk) and reproduce byte-for-byte
     * across builds, which is what lets the persistent prerender cache hit. Set
     * TAC_RANDOM_IDS to restore the legacy random-per-build ids (escape hatch).
     * @param {string} seed
     * @returns {() => string}
     */
    static idGenerator(seed) {
        if (process.env.TAC_RANDOM_IDS)
            return () => Bun.randomUUIDv7().replace(/-/g, '').slice(-8);
        const prefix = Compiler.deterministicHash(seed);
        let counter = 0;
        return () => `${prefix}${(counter++).toString(36)}`;
    }

    /**
     * @param {string} filePath
     * @returns {RegExp}
     */
    static createFilePathFilter(filePath) {
        const absolutePath = path.resolve(filePath);
        const posixPath = absolutePath.replaceAll(path.sep, '/');
        const fileUrl = pathToFileURL(absolutePath).href;
        return new RegExp(`^(?:${Compiler.escapeRegExp(absolutePath)}|${Compiler.escapeRegExp(posixPath)}|${Compiler.escapeRegExp(fileUrl)})$`);
    }
    /** @returns {OutputFormat} */
    static resolveOutputFormat() {
        const value = (process.env.TAC_FORMAT || 'esm').toLowerCase();
        return value === 'global' || value === 'umd' || value === 'iife' ? 'global' : 'esm';
    }
    /**
     * @returns {import('../shared/native-targets.js').BundleTarget[]}
     */
    static bundleTargets() {
        return /** @type {import('../shared/native-targets.js').BundleTarget[]} */ ((process.env.TAC_BUNDLE_TARGETS || 'web')
            .split(',')
            .map((target) => target.trim())
            .filter(Boolean));
    }
    /**
     * Safe capabilities are enabled by default. Raw filesystem and shell
     * capabilities are supplied by the validated package configuration for
     * the active build.
     * @returns {string[]}
     */
    static nativeCapabilities() {
        const requested = (process.env.TAC_NATIVE_CAPABILITIES ?? '')
            .split(',')
            .map((capability) => capability.trim())
            .filter(Boolean);
        return [...nativeHostCapabilities(Compiler.currentBundleTarget(), requested)].sort();
    }
    /**
     * Returns true when the current bundle target is a native platform.
     * @returns {boolean}
     */
    static isNativeBundle() {
        const targets = Compiler.bundleTargets();
        return targets.length === 1 && isNativeTarget(targets[0]);
    }
    /**
     * The single bundle target currently being built. During `tac.bundle` each
     * target is built in its own pass with `TAC_BUNDLE_TARGETS` set to one
     * value, so this returns that value. Falls back to 'web'.
     * @returns {import('../shared/native-targets.js').BundleTarget}
     */
    static currentBundleTarget() {
        return Compiler.bundleTargets()[0] ?? 'web';
    }
    /**
     * Runtime target context for the bundle currently being emitted.
     * @returns {import('../shared/native-targets.js').BundleTargetContext}
     */
    static currentTargetContext() {
        return targetContext(Compiler.currentBundleTarget());
    }
    /**
     * Asset base prefix for the current target. Native apps load from the
     * local file system, so assets must be relative to index.html. Web targets
     * keep absolute paths so they work from any route.
     * @returns {string}
     */
    static assetBasePath() {
        return Compiler.isNativeBundle() ? './' : '/';
    }
    /**
     * Converts an absolute public path (e.g. /components/foo.js) into the
     * correct form for the current target.
     * @param {string} publicPath
     * @returns {string}
     */
    static assetPath(publicPath) {
        if (!publicPath.startsWith('/'))
            return publicPath;
        const base = Compiler.assetBasePath();
        return `${base}${publicPath.slice(1)}`;
    }
    /**
     * Escapes a package name so it can be used as a single URL path segment.
     * Scoped packages such as `@scope/name` become `scope+name`.
     * @param {string} moduleName
     * @returns {string}
     */
    static moduleRouteKey(moduleName) {
        return moduleName.replace('/', '+');
    }
    /**
     * Public URL path for a bundled npm dependency module.
     * @param {string} moduleName
     * @returns {string}
     */
    static moduleRoutePath(moduleName) {
        return `${MODULES_ROUTE_PREFIX}/${Compiler.moduleRouteKey(moduleName)}.js`;
    }
    /**
     * Returns a relative import path from one module to another. ES module
     * imports are resolved relative to the importing module file, so this is
     * required for both web and native bundles. It also lets the generated
     * modules work when imported directly from the filesystem (e.g. tests).
     * @param {string} fromPublicPath
     * @param {string} toPublicPath
     * @returns {string}
     */
    static relativeModuleImportPath(fromPublicPath, toPublicPath) {
        const from = fromPublicPath.startsWith('/') ? fromPublicPath.slice(1) : fromPublicPath;
        const to = toPublicPath.startsWith('/') ? toPublicPath.slice(1) : toPublicPath;
        let relative = path.posix.relative(path.posix.dirname(from), to);
        if (!relative.startsWith('.'))
            relative = `./${relative}`;
        return relative;
    }
    static isGlobalOutput() {
        return Compiler.outputFormat === 'global';
    }

    /**
     * @param {BunRequest} request
     * @param {string} route
     * @returns {{ params: ReturnType<typeof Router.parseParams> }}
     */
    static getParams(request, route) {
        const url = new URL(request.url);
        const params = url.pathname.split('/').slice(route.split('/').length);
        return { params: Router.parseParams(params) };
    }

    /** @param {string} route */
    static normalizeComponentName(route) {
        const segments = route.replace('.html', '').split('/').filter(Boolean);
        if (segments[segments.length - 1] === 'tac')
            segments.pop();
        return segments.join('-').toLowerCase();
    }

    /** @param {string} route */
    static validateComponentRoute(route) {
        const normalized = route.replaceAll('\\', '/');
        const segments = normalized.split('/');
        const isTacTemplate = segments.at(-1) === Router.pageFileName;
        const validSegment = /^[a-z][a-z0-9]*$/;
        const hasValidName = segments.length >= 2
            && segments.slice(0, -1).every((segment) => validSegment.test(segment));
        if (isTacTemplate && hasValidName)
            return;
        throw new Error([
            `Invalid Tac component path '${route}'.`,
            `Components must use lowercase alphanumeric folders with a ${Router.pageFileName} template.`,
            `Example: client/components/user/card/${Router.pageFileName} is used as <user-card />.`,
        ].join(' '));
    }

    /** @param {string} tagName */
    static classifyElementTag(tagName) {
        const normalized = tagName.toLowerCase();
        if (Compiler.compMapping.has(normalized))
            return { kind: 'component', name: normalized };
        if (Compiler.isWebComponentTag(normalized))
            return { kind: 'web-component' };
        if (Compiler.nativeTags.has(normalized))
            return { kind: 'native' };
        if (Compiler.controlTags.has(normalized))
            return { kind: 'control' };
        return { kind: 'unknown' };
    }

    /** @param {string} tagName */
    static isWebComponentTag(tagName) {
        return /^[a-z][.0-9_a-z]*-[.0-9_a-z-]*$/.test(tagName)
            && !Compiler.reservedCustomElementNames.has(tagName);
    }

    /** @param {string} expr */
    static compileLoopExpression(expr) {
        const trimmed = expr.trim();
        if (/^(?:let|const|var)\s/.test(trimmed))
            return `for(${trimmed}) {`;
        const forOfMatch = trimmed.match(/^([a-zA-Z_$][\w$]*)\s+of\s+([\s\S]+)$/);
        if (forOfMatch)
            return `for(let ${forOfMatch[1]} of ${forOfMatch[2]}) {`;
        const forInMatch = trimmed.match(/^([a-zA-Z_$][\w$]*)\s+in\s+([\s\S]+)$/);
        if (forInMatch)
            return `for(let ${forInMatch[1]} in ${forInMatch[2]}) {`;
        return `for(${trimmed}) {`;
    }
    /** @param {string} source */
    static isControlElementSource(source) {
        return /<\/?(?:loop|logic|switch|case)(?:\s|>|`)/.test(source);
    }
    /**
     * @param {unknown} switchValue
     * @param {unknown} caseValue
     */
    static matchSwitchCase(switchValue, caseValue) {
        return Array.isArray(caseValue)
            ? caseValue.some((value) => Object.is(value, switchValue))
            : Object.is(caseValue, switchValue);
    }
    /**
     * @param {string} expression
     * @returns {string | null}
     */
    static getStaticCaseKey(expression) {
        const trimmed = expression.trim();
        if (/^(['"]).*\1$/.test(trimmed))
            return `string:${trimmed.slice(1, -1)}`;
        if (/^-?\d+(?:\.\d+)?$/.test(trimmed))
            return `number:${trimmed}`;
        if (trimmed === 'true' || trimmed === 'false')
            return `boolean:${trimmed}`;
        if (trimmed === 'null')
            return 'null:null';
        return null;
    }
    /**
     * @param {string} renderSource
     * @param {string} sourceName
     */
    static compileSwitchExpressions(renderSource, sourceName) {
        /** @type {{ valueVar: string, matchedVar: string, defaultSeen: boolean, staticCases: Set<string>, caseClosers: string[] }[]} */
        const stack = [];
        let index = 0;
        const pattern = /`<switch\s+:value="([\s\S]*?)">`|`<\/switch>`|`<case\s+:when="([\s\S]*?)">`|`<case\s+default="">`|`<\/case>`/g;
        const output = renderSource.replaceAll(pattern, (match, switchExpr, caseExpr) => {
            if (switchExpr !== undefined) {
                const id = index++;
                const frame = {
                    valueVar: `__tc_switch_value_${id}`,
                    matchedVar: `__tc_switch_matched_${id}`,
                    defaultSeen: false,
                    staticCases: new Set(),
                    caseClosers: [],
                };
                stack.push(frame);
                return `{ const ${frame.valueVar}=(${switchExpr}); let ${frame.matchedVar}=false;`;
            }
            if (match === '`</switch>`') {
                if (stack.length === 0)
                    throw new Error(`Invalid Tac switch in '${sourceName}': </switch> has no matching <switch>.`);
                stack.pop();
                return '}';
            }
            const frame = stack.at(-1);
            if (!frame)
                throw new Error(`Invalid Tac switch in '${sourceName}': <case> must be inside <switch>.`);
            if (caseExpr !== undefined) {
                const staticKey = Compiler.getStaticCaseKey(caseExpr);
                if (staticKey) {
                    if (frame.staticCases.has(staticKey))
                        throw new Error(`Invalid Tac switch in '${sourceName}': duplicate literal case value '${caseExpr}'.`);
                    frame.staticCases.add(staticKey);
                }
                frame.caseClosers.push('}}');
                return `{ const __tc_case_value=(${caseExpr}); if(!${frame.matchedVar} && __tc_helpers__.matchSwitchCase(${frame.valueVar}, __tc_case_value)) { ${frame.matchedVar}=true;`;
            }
            if (match === '`<case default="">`' || match === '`<case default>`') {
                if (frame.defaultSeen)
                    throw new Error(`Invalid Tac switch in '${sourceName}': only one default case is allowed per <switch>.`);
                frame.defaultSeen = true;
                frame.caseClosers.push('}');
                return `if(!${frame.matchedVar}) { ${frame.matchedVar}=true;`;
            }
            const closer = frame.caseClosers.pop();
            if (!closer)
                throw new Error(`Invalid Tac switch in '${sourceName}': </case> has no matching <case>.`);
            return closer;
        });
        if (stack.length > 0)
            throw new Error(`Invalid Tac switch in '${sourceName}': <switch> is missing </switch>.`);
        if (output.includes('`<switch') || output.includes('`<case'))
            throw new Error(`Invalid Tac switch in '${sourceName}': use <switch :value="..."> and <case :when="..."> or <case default>.`);
        return output;
    }

    /**
     * @param {string} tagName
     * @param {string} sourceName
     */
    static warnUnknownTag(tagName, sourceName) {
        const key = `${sourceName}:${tagName}`;
        if (Compiler.warnedUnknownTags.has(key))
            return;
        Compiler.warnedUnknownTags.add(key);
        Compiler.compilerLogger.warn('Unknown element tag; treating it as plain HTML', {
            tag: tagName,
            source: sourceName,
        });
    }

    /**
     * A bare native handler (`onclick`, `oninput`, …) on a Tac template is almost
     * always a typo for the `on:<event>` directive. It silently no-ops against
     * Tac's delegation (and double-binds under Light-DOM components), so warn and
     * point at the fix rather than emitting it as a raw attribute.
     * @param {string} name @param {string} tagName @param {string} sourceName
     */
    static warnBareHandler(name, tagName, sourceName) {
        const key = `${sourceName}:${tagName}:${name}`;
        if (Compiler.warnedBareHandlers.has(key))
            return;
        Compiler.warnedBareHandlers.add(key);
        Compiler.compilerLogger.warn(`Bare handler "${name}" will not be wired by Tac; did you mean "on:${name.slice(2)}"?`, {
            attribute: name,
            suggestion: `on:${name.slice(2)}`,
            tag: tagName,
            source: sourceName,
        });
    }

    /**
     * The `<loop>`/`<logic>` regex passes only match well-formed directives
     * (`:for`, `:if`, `:else-if`, `else`). A surviving `<loop>`/`<logic>` literal
     * means the author typo'd or omitted the directive — it would otherwise leak
     * into the DOM as text. Turn that silent leak into a located, named error.
     * @param {string} renderSource @param {string} sourceName
     */
    static assertControlDirectivesResolved(renderSource, sourceName) {
        const leak = renderSource.match(/`<(loop|logic)\b([^`]*)>`/);
        if (!leak)
            return;
        const [, tag, rawAttrs] = leak;
        const got = `<${tag}${rawAttrs}>`;
        if (tag === 'loop')
            throw new Error(`Invalid Tac <loop> in '${sourceName}': expected a :for directive like <loop :for="item of items">, got \`${got}\`.`);
        throw new Error(`Invalid Tac <logic> in '${sourceName}': expected :if, :else-if, or else like <logic :if="cond">, got \`${got}\`.`);
    }

    /**
     * Validate an interpolation expression's syntax at compile time, so a typo
     * like `{ user.nam( }` fails the build with the offending expression quoted —
     * instead of a cryptic parse error surfacing from the prerender worker at
     * render time. Only syntax is checked; free identifiers (state, props) resolve
     * at render. Wrapped in an async arrow so interpolations may legitimately await.
     * @param {string} expr @param {string} sourceName
     */
    static assertValidInterpolation(expr, sourceName) {
        if (!expr.trim())
            return;
        try {
            // eslint-disable-next-line no-new-func
            new Function(`"use strict"; return (async () => (\n${expr}\n));`);
        }
        catch {
            throw new Error(`Invalid Tac interpolation in '${sourceName}': {${expr}} is not a valid expression.`);
        }
    }
    static async createStaticRoutes() {
        Compiler.compMapping.clear();
        Compiler.wrapperPages = {};
        Compiler.warnedUnknownTags.clear();
        Compiler.warnedBareHandlers.clear();
        await Router.validatePageRoutes();
        // Components must be registered before pages compile so that
        // compMapping is fully populated when import statements are generated.
        await Compiler.bundleComponents();
        await Compiler.bundlePages();
        await Compiler.bundleBrowserRuntimeAssets();
        await Promise.all([
            Compiler.bundleDependencies(),
            Compiler.bundleAssets(),
            Compiler.bundleSharedData()
        ]);
    }

    /** @param {string} html */
    static normalizeScopedStyles(html) {
        return html.replace(/<style>([\s\S]*?)<\/style>/g, (_match, css) => {
            const trimmed = css.trim();
            if (trimmed === "@scope {  }" || trimmed === "@scope {}") {
                return "";
            }
            if (trimmed.startsWith("@scope {") && trimmed.endsWith("}")) {
                const inner = trimmed.slice("@scope {".length, -1).trim();
                return `<style>${inner}</style>`;
            }
            return `<style>${css}</style>`;
        });
    }

    /** @param {string} value */
    static escapeHTML(value) {
        return value
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#39;");
    }

    /**
     * @param {string} pathname
     * @returns {string | null}
     */
    static resolveWrapperPage(pathname) {
        if (pathname !== '/') {
            const segments = pathname.split('/').filter(Boolean);
            for (let count = segments.length; count > 0; count -= 1) {
                const prefix = `/${segments.slice(0, count).join('/')}`;
                const entry = Compiler.wrapperPages[prefix];
                if (entry && (entry.allowSelf || prefix !== pathname))
                    return entry.path;
            }
        }
        const rootEntry = Compiler.wrapperPages['/'];
        if (rootEntry && (rootEntry.allowSelf || pathname !== '/'))
            return rootEntry.path;
        return null;
    }

    /**
     * @param {string} filePath
     * @param {string} distPath
     * @returns {Promise<void>}
     */
    static async rewriteAbsoluteImports(filePath, distPath) {
        const source = await readFile(filePath, 'utf8');
        const fileDir = path.dirname(filePath);
        const rewritten = source.replace(/import\("(?<spec>\/(?:components|shared\/modules)\/[^"]+)"\)/g, (_match, spec) => {
            const relative = path.relative(fileDir, path.join(distPath, spec.slice(1))).replaceAll(path.sep, '/');
            const normalized = relative.startsWith('.') ? relative : `./${relative}`;
            return `import("${normalized}")`;
        });
        if (rewritten !== source)
            await writeFile(filePath, rewritten);
    }

    /** @param {string} filePath */
    static isMainEntrypoint(filePath) {
        return Compiler.mainEntryCandidates.includes(path.posix.basename(filePath));
    }
    static getMainEntryCandidates(cwd = process.cwd()) {
        const configuredSharedScriptsPath = process.env.YON_SHARED_SCRIPTS_PATH;
        const sharedScriptsBase = configuredSharedScriptsPath
            ? path.isAbsolute(configuredSharedScriptsPath)
                ? configuredSharedScriptsPath
                : path.join(cwd, configuredSharedScriptsPath)
            : path.join(cwd, 'client', 'shared', 'scripts');
        const candidates = [
            ...Compiler.mainEntryCandidates.map((entry) => path.join(cwd, entry)),
            ...Compiler.mainEntryCandidates.map((entry) => path.join(sharedScriptsBase, entry)),
        ];
        return [...new Set(candidates)];
    }
    static async getMainEntrypointPath(cwd = process.cwd()) {
        for (const candidate of Compiler.getMainEntryCandidates(cwd)) {
            if (await Bun.file(candidate).exists())
                return candidate;
        }
        return null;
    }

    /** @param {string} outputPath */
    static routePathFromBuildOutput(outputPath) {
        const normalized = outputPath.replaceAll(path.sep, '/').replace(/^\.\//, '');
        return normalized.startsWith('/') ? normalized : `/${normalized}`;
    }
    static getBrowserRuntimeRoutes() {
        return [...Compiler.browserRuntimeRoutes];
    }

    /**
     * @param {string} route
     * @returns {string}
     */
    static routePathFromPageSource(route) {
        const pageDir = Router.filesystemPathToRoute(path.posix.dirname(route));
        return pageDir === '.' ? '/' : `/${pageDir}`;
    }

    /**
     * @param {string} pathname
     * @returns {string}
     */
    static pageModulePublicPath(pathname) {
        return pathname === '/'
            ? Compiler.assetPath('/pages/tac.js')
            : Compiler.assetPath(`/pages${Router.routeToFilesystemPath(pathname)}/tac.js`);
    }

    /**
     * @param {string} distPath
     * @param {string} pathname
     * @returns {string}
     */
    static pageModuleFilePath(distPath, pathname) {
        return pathname === '/'
            ? path.join(distPath, 'pages', 'tac.js')
            : path.join(distPath, 'pages', Router.routeToFilesystemPath(pathname).slice(1), 'tac.js');
    }

    /** @param {string | undefined} scriptLang */
    static normalizeScriptLoader(scriptLang) {
        const normalized = (scriptLang || 'js').toLowerCase();
        if (normalized === 'tsx')
            return 'tsx';
        if (normalized === 'ts')
            return 'ts';
        if (normalized === 'jsx')
            return 'jsx';
        return 'js';
    }

    /** @param {string} filePath */
    static loaderForFilePath(filePath) {
        const extension = path.extname(filePath).toLowerCase();
        if (extension === '.tsx')
            return 'tsx';
        if (extension === '.ts')
            return 'ts';
        if (extension === '.jsx')
            return 'jsx';
        return 'js';
    }

    /** @param {string} route */
    static toModuleOutputRoute(route) {
        return route.replace(/\.(?:html|[cm]?[jt]sx?)$/, '.js');
    }

    /** @param {string} sourcePath */
    static toSourceLabel(sourcePath) {
        const relative = path.relative(process.cwd(), sourcePath).replaceAll(path.sep, '/');
        return relative.startsWith('.') ? relative : `./${relative}`;
    }
    /** @param {string} sourcePath */
    static templateBasePath(sourcePath) {
        return sourcePath.endsWith('.html')
            ? sourcePath.slice(0, -'.html'.length)
            : sourcePath;
    }
    /**
     * Tac companion artifacts are deliberately target-neutral. Each selected
     * native host implements the device bridge, so a language is never tied to
     * the operating system used to package the final application.
     * @param {CompanionProvider} provider
     * @param {string} _target
     */
    static companionProviderSupportsTarget(provider, _target = Compiler.currentBundleTarget()) {
        return provider.portable;
    }
    /** @param {string} sourcePath */
    static async getCompanionScript(sourcePath) {
        const basePath = Compiler.templateBasePath(sourcePath);
        for (const provider of Compiler.companionProviders) {
            const candidate = `${basePath}${provider.extension}`;
            if (await Bun.file(candidate).exists()) {
                return {
                    sourcePath: candidate,
                    importPath: Compiler.toRelativeImportPath(sourcePath, candidate),
                    provider,
                };
            }
        }
        return null;
    }
    /** @param {string} sourcePath */
    static async getCompanionScriptPath(sourcePath) {
        return (await Compiler.getCompanionScript(sourcePath))?.sourcePath ?? null;
    }
    /** @param {string} sourcePath */
    static async getCompanionStylePath(sourcePath) {
        const candidate = `${Compiler.templateBasePath(sourcePath)}.css`;
        return await Bun.file(candidate).exists() ? candidate : null;
    }
    /** @param {string} sourcePath @param {string} targetPath */
    static toRelativeImportPath(sourcePath, targetPath) {
        const relative = path.relative(path.dirname(sourcePath), targetPath).replaceAll(path.sep, '/');
        return relative.startsWith('.') ? relative : `./${relative}`;
    }
    /** @param {string} source */
    static maskCommentsAndStrings(source) {
        return source
            .replace(/\/\*[\s\S]*?\*\//g, (match) => ' '.repeat(match.length))
            .replace(/\/\/.*$/gm, (match) => ' '.repeat(match.length))
            .replace(/(['"`])(?:\\.|(?!\1).)*\1/g, (match) => ' '.repeat(match.length));
    }
    /** @param {string} source */
    static shouldInjectTacImport(source) {
        const stripped = Compiler.maskCommentsAndStrings(source);
        return !(/^\s*import\s+(?:type\s+)?(?:\{[^}]*\bTac\b[^}]*\}|\bTac\b)/m.test(stripped)
            || /^\s*(?:const|let|var|class|function)\s+Tac\b/m.test(stripped));
    }
    /** @param {string} source */
    static injectTacBaseClass(source) {
        const masked = Compiler.maskCommentsAndStrings(source);
        const match = /(^|\n)([ \t]*export\s+default\s+class\b[^\n{]*)(\{)/m.exec(masked);
        if (!match || /\bextends\b/.test(match[2])) return source;
        const insertAt = match.index + match[1].length + match[2].length;
        const before = source.slice(0, insertAt).replace(/[ \t]*$/, '');
        return Compiler.injectTacConstructorSuper(`${before} extends Tac ${source.slice(insertAt)}`);
    }
    /** @param {string} source @param {number} openBraceIndex */
    static findMatchingBrace(source, openBraceIndex) {
        let depth = 0;
        for (let index = openBraceIndex; index < source.length; index += 1) {
            const char = source[index];
            if (char === '{') depth += 1;
            if (char === '}') {
                depth -= 1;
                if (depth === 0) return index;
            }
        }
        return -1;
    }
    /** @param {string} source */
    static injectTacConstructorSuper(source) {
        const masked = Compiler.maskCommentsAndStrings(source);
        const classMatch = /(^|\n)[ \t]*export\s+default\s+class\b[^\n{]*\bextends\s+Tac\b[^\n{]*(\{)/m.exec(masked);
        if (!classMatch) return source;
        const classOpen = classMatch.index + classMatch[0].lastIndexOf('{');
        const classClose = Compiler.findMatchingBrace(masked, classOpen);
        if (classClose === -1) return source;
        const classBody = masked.slice(classOpen + 1, classClose);
        const constructorMatch = /(^|\n)([ \t]*)constructor\s*\([^)]*\)\s*\{/m.exec(classBody);
        if (!constructorMatch) return source;
        const constructorOpen = classOpen + 1 + constructorMatch.index + constructorMatch[0].lastIndexOf('{');
        const constructorClose = Compiler.findMatchingBrace(masked, constructorOpen);
        if (constructorClose === -1) return source;
        const constructorBody = masked.slice(constructorOpen + 1, constructorClose);
        if (/\bsuper\s*\(/.test(constructorBody)) return source;
        const indent = `${constructorMatch[2]}    `;
        return `${source.slice(0, constructorOpen + 1)}\n${indent}super(...arguments);${source.slice(constructorOpen + 1)}`;
    }
    /**
     * Returns the subset of supported decorator names that the source uses as
     * decorations (`@<name>` outside of comments and string literals) and that
     * are not already imported or locally declared.
     * @param {string} source
     * @returns {string[]}
     */
    static findReferencedDecorators(source) {
        const stripped = Compiler.maskCommentsAndStrings(source);
        const referenced = [];
        for (const name of Compiler.companionDecoratorNames) {
            const usagePattern = new RegExp(`(?:^|[^@\\w])@${name}\\b`, 'm');
            if (!usagePattern.test(stripped)) continue;
            const importPattern = new RegExp(`^\\s*import\\s+[\\s\\S]*?\\b${name}\\b[\\s\\S]*?\\bfrom\\b`, 'm');
            const declarationPattern = new RegExp(`^\\s*(?:const|let|var|class|function)\\s+${name}\\b`, 'm');
            if (importPattern.test(stripped) || declarationPattern.test(stripped)) continue;
            referenced.push(name);
        }
        return referenced;
    }
    /**
     * Throws when a companion script references decorators removed from the
     * strict v2 Tac API, unless the app intentionally imports or declares its
     * own symbol with that name.
     * @param {string} source
     * @param {string} sourcePath
     */
    static assertNoRemovedDecorators(source, sourcePath) {
        const stripped = Compiler.maskCommentsAndStrings(source);
        for (const name of Compiler.removedCompanionDecoratorNames) {
            const usagePattern = new RegExp(`(?:^|[^@\\w])@${name}\\b`, 'm');
            if (!usagePattern.test(stripped)) continue;
            const importPattern = new RegExp(`^\\s*import\\s+[\\s\\S]*?\\b${name}\\b[\\s\\S]*?\\bfrom\\b`, 'm');
            const declarationPattern = new RegExp(`^\\s*(?:const|let|var|class|function)\\s+${name}\\b`, 'm');
            if (importPattern.test(stripped) || declarationPattern.test(stripped)) continue;
            throw new Error(`Tac decorator @${name} is not supported in v2. Reassign instance fields to trigger automatic rerenders instead. (${Compiler.toSourceLabel(sourcePath)})`);
        }
    }
    /**
     * Detects bare `fylo` references in a companion script (e.g. `fylo.users.find(...)`).
     * Returns true only if the script uses it without already importing or
     * declaring the symbol. The compiler prepends an import from
     * `runtime/fylo-global.js` when this returns true — same pattern used for
     * decorators and Tac.
     * @param {string} source
     * @returns {boolean}
     */
    static referencesFyloGlobal(source) {
        const stripped = Compiler.maskCommentsAndStrings(source);
        const usagePattern = /(?:^|[^.\w@])fylo\b/m;
        if (!usagePattern.test(stripped)) return false;
        const importPattern = /^\s*import\s+[\s\S]*?\bfylo\b[\s\S]*?\bfrom\b/m;
        const declarationPattern = /^\s*(?:const|let|var|class|function)\s+fylo\b/m;
        if (importPattern.test(stripped) || declarationPattern.test(stripped)) return false;
        return true;
    }
    /**
     * Non-JavaScript companions use the implicit `fylo` facade. Their generated
     * module imports the bundled browser client for its window registration side
     * effect; it never resolves a package from npm.
     * @param {string} source
     */
    static referencesFyloFacade(source) {
        return /\b(?:Fylo|fylo)\b/.test(Compiler.maskCommentsAndStrings(source));
    }
    /** @param {string} css @param {string} dir @param {string} route */
    static buildCompanionStyle(css, dir, route) {
        if (dir !== 'components')
            return css;
        const scopeName = Compiler.normalizeComponentName(route);
        return `@scope ([data-tac-scope="${scopeName}"]) {\n${css}\n}`;
    }
    /** @param {string} scriptContent @param {string | undefined} scriptLang */
    static async transpileInlineScript(scriptContent, scriptLang) {
        if (!scriptContent)
            return '';
        const loader = Compiler.normalizeScriptLoader(scriptLang);
        if (loader === 'js')
            return scriptContent;
        return new Bun.Transpiler({ loader }).transformSync(scriptContent);
    }
    /**
     * @param {string} scriptContent
     * @returns {{ bindingNames: string[], moduleImports: string[], scriptContent: string }}
     */
    static liftDynamicImports(scriptContent) {
        let index = 0;
        /** @type {string[]} */
        const moduleImports = [];
        /** @type {string[]} */
        const bindingNames = [];
        const transformed = scriptContent.replace(/import\((['"][^'"]+['"])\)/g, (_match, specifier) => {
            const helperName = `__tc_dynamic_import_${index++}__`;
            moduleImports.push(`${helperName}: () => import(${specifier})`);
            bindingNames.push(helperName);
            return `${helperName}()`;
        });
        return {
            bindingNames,
            moduleImports,
            scriptContent: transformed,
        };
    }
    /**
     * @param {string} shellHTML
     * @param {string} [assetPrefix]
     */
    static withMainStylesheet(shellHTML, assetPrefix = Compiler.assetBasePath()) {
        const href = `${assetPrefix}imports.css`;
        if (!Router.reqRoutes['/imports.css'] || shellHTML.includes(href))
            return shellHTML;
        return shellHTML.replace('</head>', `    <link rel="stylesheet" href="${href}">\n</head>`);
    }
    /** @returns {Promise<string>} */
    static async faviconPath() {
        const candidates = ['favicon.svg', 'app-icon.svg'];
        for (const candidate of candidates) {
            if (await pathExists(path.join(Router.assetsPath, candidate)))
                return `/shared/assets/${candidate}`;
        }
        if (Compiler.isNativeBundle())
            return '/shared/assets/favicon.svg';
        return '';
    }
    /**
     * @param {string} shellHTML
     * @param {string} [assetPrefix]
     */
    static async withFavicon(shellHTML, assetPrefix = Compiler.assetBasePath()) {
        if (shellHTML.includes('rel="icon"'))
            return shellHTML;
        const favicon = await Compiler.faviconPath();
        if (!favicon)
            return shellHTML;
        const href = `${assetPrefix}${favicon.slice(1)}`;
        return shellHTML.replace('</head>', `    <link rel="icon" type="image/svg+xml" href="${href}">\n    <link rel="apple-touch-icon" href="${href}">\n</head>`);
    }
    /**
     * Locate an app-provided web app manifest in the shared assets folder.
     * @returns {Promise<string>} Public path, or '' when the app ships none.
     */
    static async webAppManifestPath() {
        const candidates = ['manifest.webmanifest', 'manifest.json'];
        for (const candidate of candidates) {
            if (await pathExists(path.join(Router.assetsPath, candidate)))
                return `/shared/assets/${candidate}`;
        }
        return '';
    }
    /**
     * Link the app's web app manifest (PWA) when one exists under
     * `client/shared/assets/`, mirroring the favicon convention. The
     * manifest's `theme_color` is surfaced as a `<meta name="theme-color">`.
     * @param {string} shellHTML
     * @param {string} [assetPrefix]
     */
    static async withWebAppManifest(shellHTML, assetPrefix = Compiler.assetBasePath()) {
        if (shellHTML.includes('rel="manifest"'))
            return shellHTML;
        const manifest = await Compiler.webAppManifestPath();
        if (!manifest)
            return shellHTML;
        const href = `${assetPrefix}${manifest.slice(1)}`;
        let themeColorMeta = '';
        try {
            const parsed = JSON.parse(await Bun.file(path.join(Router.assetsPath, path.basename(manifest))).text());
            if (typeof parsed.theme_color === 'string' && /^[#a-zA-Z0-9(),.% -]+$/.test(parsed.theme_color))
                themeColorMeta = `    <meta name="theme-color" content="${parsed.theme_color}">\n`;
        }
        catch {
            // Malformed manifest: still link it, skip the theme-color meta.
        }
        return shellHTML.replace('</head>', `    <link rel="manifest" href="${href}">\n${themeColorMeta}</head>`);
    }
    /** @type {Promise<string> | null} */
    static #clientContentHashPromise = null;
    /**
     * Content hash of the client source tree. Folded into the cache version so
     * the service worker evicts old caches on any rebuild whose inputs
     * changed — not just when routes or the app version change (same-version
     * redeploys used to serve stale assets from the Cache API indefinitely).
     * Cached per process: one bundle run hashes once, and the dev server
     * (where sources change live) never registers the service worker.
     * @returns {Promise<string>}
     */
    static clientContentHash() {
        return Compiler.#clientContentHashPromise ??= (async () => {
            const roots = [
                Router.pagesPath,
                Router.componentsPath,
                Router.assetsPath,
                Router.sharedDataPath,
                Router.sharedScriptsPath,
                Router.sharedStylesPath,
            ];
            /** @type {string[]} */
            const files = [];
            for (const root of roots) {
                if (!existsSync(root))
                    continue;
                for (const entry of await readdir(root, { recursive: true, withFileTypes: true })) {
                    if (entry.isFile())
                        files.push(path.join(entry.parentPath, entry.name));
                }
            }
            files.sort();
            let digest = '';
            for (const file of files)
                digest += `${file}:${Bun.hash(await Bun.file(file).arrayBuffer()).toString(36)};`;
            return Bun.hash(digest).toString(36).slice(0, 10) || '0';
        })();
    }
    /**
     * @param {{ includeHotReloadClient?: boolean }} [options]
     * @returns {Promise<string>}
     */
    static async renderShellHTML(options = {}) {
        const includeHotReloadClient = options.includeHotReloadClient === true;
        const fyloBrowserPath = process.env.YON_DATA_BROWSER_PATH || '/_fylo';
        const assetPrefix = Compiler.assetBasePath();
        // Cache version — routes hash plus a client-source content hash, so
        // the service worker evicts old caches on any rebuild that changed
        // inputs, not only when routes change.
        const version = `${Router.routeManifestHash()}-${await Compiler.clientContentHash()}`;
        const importMap = await Compiler.generateImportMap();
        const targetInfo = Compiler.currentTargetContext();
        const importMapScript = Object.keys(importMap).length > 0
            ? `    <script type="importmap">${JSON.stringify({ imports: importMap })}</script>\n`
            : '';
        const nativeCapabilities = Compiler.nativeCapabilities()
            .join(',')
            .replaceAll('&', '&amp;')
            .replaceAll('"', '&quot;')
            .replaceAll('<', '&lt;');
        let shellHTML = await frameworkText('runtime/shells/app.html');
        shellHTML = shellHTML
            // Pre-hydration capture: classic inline script runs before the deferred
            // runtime module, recording dead-zone click/submit interactions for replay.
            .replace('<head>', `<head>\n    <script>${EVENT_CAPTURE_SCRIPT}</script>`)
            .replace(IMPORT_MAP_PLACEHOLDER, importMapScript)
            .replace('<!--__TACHYON_DEV_HEAD__-->', includeHotReloadClient
                ? `    <script type="module" src="${assetPrefix}hot-reload-client.js"></script>`
                : '')
            .replace('src="/spa-renderer.js"', `src="${assetPrefix}spa-renderer.js"`)
            .replace('src="/imports.js"', `src="${assetPrefix}imports.js"`)
            .replace('__FYLO_BROWSER_PATH__', fyloBrowserPath)
            .replace('</head>', [
                `    <meta name="tachyon-version" content="${version}">`,
                `    <meta name="tachyon-target" content="${targetInfo.target}">`,
                `    <meta name="tachyon-platform" content="${targetInfo.platform}">`,
                `    <meta name="tachyon-environment" content="${targetInfo.environment}">`,
                `    <meta name="tachyon-os" content="${targetInfo.os}">`,
                `    <meta name="tachyon-native-capabilities" content="${nativeCapabilities}">`,
                '</head>',
            ].join('\n'));
        shellHTML = Compiler.withMainStylesheet(shellHTML, assetPrefix);
        shellHTML = await Compiler.withFavicon(shellHTML, assetPrefix);
        shellHTML = await Compiler.withWebAppManifest(shellHTML, assetPrefix);
        return withPublicBrowserEnv(shellHTML, assetPrefix);
    }
    /**
     * Stages an SPA renderer with its route metadata already substituted.
     * Bun plugins are not reliably invoked from a compiled executable, so this
     * keeps the runtime bundle identical for source and standalone `ty` use.
     *
     * @param {string} spaRendererPath
     * @returns {Promise<{ entrypoint: string, root: string }>}
     */
    static async stageSpaRendererEntrypoint(spaRendererPath) {
        const routeManifestJSON = JSON.stringify(Router.routeSlugs);
        const wrapperManifestJSON = JSON.stringify(Compiler.wrapperPages);
        const assetPrefixLiteral = Compiler.isNativeBundle() ? './' : '/';
        const escapedRouteManifestJSON = routeManifestJSON
            .replaceAll('\\', '\\\\')
            .replaceAll("'", "\\'");
        const escapedWrapperManifestJSON = wrapperManifestJSON
            .replaceAll('\\', '\\\\')
            .replaceAll("'", "\\'");
        const source = await Bun.file(spaRendererPath).text();
        if (!source.includes(ROUTE_MANIFEST_PLACEHOLDER))
            throw new Error('Tac SPA renderer route manifest placeholder is missing');
        if (!source.includes(WRAPPER_MANIFEST_PLACEHOLDER))
            throw new Error('Tac SPA renderer wrapper manifest placeholder is missing');
        if (!source.includes('__TACHYON_ASSET_PREFIX__'))
            throw new Error('Tac SPA renderer asset prefix placeholder is missing');

        const sourceDirectory = path.dirname(spaRendererPath);
        const root = await mkdtemp(path.join(sourceDirectory, '.tachyon-spa-'));
        const rewrittenSource = source
            .replace(ROUTE_MANIFEST_PLACEHOLDER, escapedRouteManifestJSON)
            .replace(WRAPPER_MANIFEST_PLACEHOLDER, escapedWrapperManifestJSON)
            .replaceAll('__TACHYON_ASSET_PREFIX__', assetPrefixLiteral)
            // The staged entrypoint lives in a temporary directory. Point its
            // direct runtime imports back to their framework source directory.
            .replace(/(\bfrom\s+['"])\.\/([^'"]+)(['"])/g, (_match, prefix, relativePath, suffix) => {
                const resolvedPath = path.join(sourceDirectory, relativePath);
                const stagedImportPath = path.relative(root, resolvedPath).split(path.sep).join('/');
                return `${prefix}${stagedImportPath.startsWith('.') ? stagedImportPath : `./${stagedImportPath}`}${suffix}`;
            });
        const entrypoint = path.join(root, 'spa-renderer.js');
        await writeFile(entrypoint, rewrittenSource);
        return { entrypoint, root };
    }
    static async bundleBrowserRuntimeAssets() {
        const spaRendererPath = await frameworkFilePath('runtime/spa-renderer.js');
        const stagedRenderer = await Compiler.stageSpaRendererEntrypoint(spaRendererPath);
        try {
            const entrypoints = [
                stagedRenderer.entrypoint,
                await frameworkFilePath('runtime/fylo-browser-worker.js'),
                await frameworkFilePath('runtime/hot-reload-client.js'),
                await frameworkFilePath('runtime/tachyon-sw.js'),
            ];
            const mainEntrypoint = await Compiler.getMainEntrypointPath();
            if (mainEntrypoint)
                entrypoints.splice(1, 0, mainEntrypoint);
            const result = await Bun.build({
                entrypoints,
                format: 'esm',
                naming: '[name].[ext]',
                minify: true,
                splitting: true,
                target: 'browser',
            });
            const registeredRoutes = new Set();
            for (const output of result.outputs) {
                const routePath = Compiler.routePathFromBuildOutput(output.path);
                Router.reqRoutes[routePath] = {
                    GET: () => typedResponse(routePath, output, output.type)
                };
                registeredRoutes.add(routePath);
            }
            Router.reqRoutes[PUBLIC_BROWSER_ENV_PATH] = { GET: createPublicBrowserEnvResponse };
            registeredRoutes.add(PUBLIC_BROWSER_ENV_PATH);
            for (const staleRoute of Compiler.browserRuntimeRoutes) {
                if (!registeredRoutes.has(staleRoute))
                    delete Router.reqRoutes[staleRoute];
            }
            Compiler.browserRuntimeRoutes = registeredRoutes;
            return [...registeredRoutes];
        }
        finally {
            await rm(stagedRenderer.root, { recursive: true, force: true });
        }
    }
    /**
     * @param {string} distPath
     * @returns {{ title: string, restore: () => void }}
     */
    static installPrerenderGlobals(distPath) {
        const previousDocument = globalThis.document;
        const previousWindow = globalThis.window;
        const previousTac = /** @type {{ Tac?: TacRegistry }} */ (globalThis).Tac;
        const previousPlatform = /** @type {{ platform?: unknown }} */ (globalThis).platform;
        const previousEnvironment = /** @type {{ environment?: unknown }} */ (globalThis).environment;
        const previousOS = /** @type {{ os?: unknown }} */ (globalThis).os;
        const previousTarget = /** @type {{ target?: unknown }} */ (globalThis).target;
        const targetInfo = Object.freeze(Compiler.currentTargetContext());
        /** @type {Map<string, Function>} */
        const modules = new Map();
        const titleState = { value: '' };
        const documentStub = {
            title: '',
            head: {
                appendChild() { return null; }
            },
            body: {
                appendChild() { return null; }
            },
            documentElement: {
                dataset: /** @type {Record<string, string>} */ ({}),
                style: /** @type {Record<string, string>} */ ({}),
                classList: {
                    add() { },
                    remove() { },
                    toggle() { return false; }
                },
                /** @param {string} name @param {string} value */
                setAttribute(name, value) {
                    this.dataset[name] = value;
                },
                /** @param {string} name */
                removeAttribute(name) {
                    delete this.dataset[name];
                }
            },
            createElement() {
                return {
                    setAttribute() { },
                    appendChild() { },
                    rel: '',
                    type: '',
                    href: ''
                };
            }
        };
        Object.defineProperty(documentStub, 'title', {
            configurable: true,
            enumerable: true,
            get() {
                return titleState.value;
            },
            set(value) {
                titleState.value = String(value);
            }
        });
        Object.assign(globalThis, {
            document: documentStub,
            window: globalThis,
            __tc_prerender__: true,
            platform: targetInfo.platform,
            environment: targetInfo.environment,
            os: targetInfo.os,
            target: targetInfo.target,
            Tac: /** @type {TacRegistry} */ ({
                version: '1',
                modules,
                platform: targetInfo,
                register(id, factory) {
                    modules.set(id, factory);
                    return factory;
                },
                async load(id) {
                    const registered = modules.get(id);
                    if (registered)
                        return registered;
                    const relative = id.startsWith('/') ? id.slice(1) : id;
                    const modulePath = pathToFileURL(path.join(distPath, relative)).href;
                    const module = await import(modulePath);
                    const loaded = modules.get(id);
                    if (loaded)
                        return loaded;
                    if (typeof module.default === 'function')
                        return module.default;
                    throw new Error(`Tac module '${id}' did not export or register a renderer`);
                }
            })
        });
        return {
            get title() {
                return titleState.value;
            },
            restore() {
                if (previousDocument === undefined)
                    Reflect.deleteProperty(globalThis, 'document');
                else
                    Object.assign(globalThis, { document: previousDocument });
                if (previousWindow === undefined)
                    Reflect.deleteProperty(globalThis, 'window');
                else
                    Object.assign(globalThis, { window: previousWindow });
                if (previousTac === undefined)
                    Reflect.deleteProperty(globalThis, 'Tac');
                else
                    Object.assign(globalThis, { Tac: previousTac });
                if (previousPlatform === undefined)
                    Reflect.deleteProperty(globalThis, 'platform');
                else
                    Object.assign(globalThis, { platform: previousPlatform });
                if (previousEnvironment === undefined)
                    Reflect.deleteProperty(globalThis, 'environment');
                else
                    Object.assign(globalThis, { environment: previousEnvironment });
                if (previousOS === undefined)
                    Reflect.deleteProperty(globalThis, 'os');
                else
                    Object.assign(globalThis, { os: previousOS });
                if (previousTarget === undefined)
                    Reflect.deleteProperty(globalThis, 'target');
                else
                    Object.assign(globalThis, { target: previousTarget });
                Reflect.deleteProperty(globalThis, '__tc_prerender__');
            }
        };
    }
    /** @param {string} filePath @param {string} publicPath */
    static async loadRenderFactoryForPrerender(filePath, publicPath) {
        const module = await import(pathToFileURL(filePath).href);
        if (typeof module.default === 'function')
            return module.default;
        const yon = /** @type {{ Tac?: TacRegistry }} */ (globalThis).Tac;
        const registered = yon?.modules?.get(publicPath);
        if (registered)
            return registered;
        throw new Error(`Tac module '${publicPath}' did not export or register a renderer`);
    }
    /** @param {string} distPath @param {string} pathname @param {string} shellHTML */
    static async renderPageDocument(distPath, pathname, shellHTML) {
        const pageFile = Compiler.pageModuleFilePath(distPath, pathname);
        const pagePublicPath = Compiler.pageModulePublicPath(pathname);
        await Compiler.rewriteAbsoluteImports(pageFile, distPath);
        const wrapperRoute = Compiler.resolveWrapperPage(pathname);
        const wrapperFile = wrapperRoute
            ? path.join(distPath, wrapperRoute.slice(1))
            : null;
        if (wrapperFile)
            await Compiler.rewriteAbsoluteImports(wrapperFile, distPath);
        const prerender = Compiler.installPrerenderGlobals(distPath);
        try {
            let wrapperHTML = '';
            if (wrapperFile) {
                const wrapperModule = await Compiler.loadRenderFactoryForPrerender(wrapperFile, /** @type {string} */ (wrapperRoute));
                const wrapperFactory = await wrapperModule();
                wrapperHTML = await wrapperFactory();
            }
            const pageModule = await Compiler.loadRenderFactoryForPrerender(pageFile, pagePublicPath);
            const pageFactory = await pageModule();
            const pageHTML = await pageFactory();
            const bodyHTML = wrapperHTML
                ? wrapperHTML.replace('<div id="tc-page-slot"></div>', () => `<div id="tc-page-slot">${pageHTML}</div>`)
                : pageHTML;
            const title = Compiler.escapeHTML(prerender.title || Compiler.routeTitleFallback);
            const withTitle = shellHTML.replace(/<title>.*?<\/title>/, () => `<title>${title}</title>`);
            const withBody = withTitle.replace('<body></body>', () => `<body>${bodyHTML}</body>`);
            return `${Compiler.normalizeScopedStyles(withBody)}\n`;
        }
        finally {
            prerender.restore();
        }
    }
    /** @param {string} distPath @param {string} pathname @param {string} shellHTML @param {WrapperPageMap} wrapperPages */
    static async renderPageDocumentForWorker(distPath, pathname, shellHTML, wrapperPages) {
        const previousWrapperPages = Compiler.wrapperPages;
        Compiler.wrapperPages = { ...wrapperPages };
        try {
            return await Compiler.renderPageDocument(distPath, pathname, shellHTML);
        }
        finally {
            Compiler.wrapperPages = previousWrapperPages;
        }
    }
    /** @param {string} distPath @param {string} pathname @param {string} shellHTML */
    static async renderPageDocumentIsolated(distPath, pathname, shellHTML) {
        if (isEmbeddedFrameworkRuntime()) {
            return Compiler.renderPageDocumentForWorker(distPath, pathname, shellHTML, Compiler.wrapperPages);
        }
        const proc = Bun.spawn(['bun', PRERENDER_WORKER_PATH], {
            cwd: process.cwd(),
            stdin: 'pipe',
            stdout: 'pipe',
            stderr: 'pipe',
        });
        proc.stdin.write(JSON.stringify({
            distPath,
            pathname,
            shellHTML,
            wrapperPages: Compiler.wrapperPages,
        }));
        proc.stdin.end();
        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]);
        if (exitCode !== 0) {
            throw new Error(stderr.trim() || `Prerender worker exited with code ${exitCode}`);
        }
        return stdout;
    }
    /** @param {string} distPath */
    static async prerenderStaticPages(distPath) {
        const htmlRoutes = Compiler.getHtmlRoutes();
        await Compiler.prerenderRoutes(distPath, htmlRoutes);
    }
    static getHtmlRoutes() {
        return Object.keys(Router.routeSlugs);
    }
    /** @param {string} distPath @param {string[]} routes */
    static async prerenderRoutes(distPath, routes) {
        const shellHTML = await Compiler.renderShellHTML();
        // Persistent prerender cache (opt-in via TAC_PRERENDER_CACHE). Sound because
        // ids are deterministic (see Compiler.idGenerator): unchanged source produces
        // identical module bytes. Keyed on the compiled page+wrapper module bytes,
        // which fully determine the rendered HTML — a page whose module is unchanged
        // since a previous build reuses that build's HTML instead of re-rendering.
        const cacheDir = process.env.TAC_PRERENDER_CACHE
            ? path.join(process.cwd(), '.tac-cache', 'prerender')
            : null;
        if (cacheDir)
            await mkdir(cacheDir, { recursive: true });
        Compiler.prerenderCacheStats = { hits: 0, misses: 0 };
        /** @type {{ outputFile: string, html: string }[]} */
        const renderedRoutes = [];
        let renderIndex = 0;
        const renderConcurrency = isEmbeddedFrameworkRuntime() ? 1 : Compiler.prerenderRenderConcurrency;
        const renderWorkers = Array.from({ length: Math.min(renderConcurrency, routes.length) }, async () => {
            while (renderIndex < routes.length) {
                const currentIndex = renderIndex++;
                const route = routes[currentIndex];
                const outputFile = route === '/'
                    ? path.join(distPath, 'index.html')
                    : path.join(distPath, Router.routeToFilesystemPath(route).slice(1), 'index.html');
                /** @type {string | null} */
                let html = null;
                /** @type {string | null} */
                let cacheFile = null;
                if (cacheDir) {
                    const key = await Compiler.prerenderCacheKey(distPath, route);
                    if (key) {
                        cacheFile = path.join(cacheDir, `${key}.html`);
                        const cached = await readFile(cacheFile, 'utf8').catch(() => null);
                        if (cached != null) {
                            // Render is skipped, but the shipped module still needs its
                            // absolute imports rewritten so the browser can load it.
                            await Compiler.rewriteModuleImportsForRoute(distPath, route);
                            html = cached;
                            Compiler.prerenderCacheStats.hits++;
                        }
                    }
                }
                if (html == null) {
                    html = routes.length === 1
                        ? await Compiler.renderPageDocument(distPath, route, shellHTML)
                        : await Compiler.renderPageDocumentIsolated(distPath, route, shellHTML);
                    if (cacheFile) {
                        await writeFile(cacheFile, html);
                        Compiler.prerenderCacheStats.misses++;
                    }
                }
                renderedRoutes[currentIndex] = { outputFile, html };
            }
        });
        await Promise.all(renderWorkers);
        let index = 0;
        const workers = Array.from({ length: Math.min(Compiler.prerenderWriteConcurrency, renderedRoutes.length) }, async () => {
            while (index < renderedRoutes.length) {
                const current = renderedRoutes[index++];
                await mkdir(path.dirname(current.outputFile), { recursive: true });
                await writeFile(current.outputFile, current.html);
            }
        });
        await Promise.all(workers);
    }
    /**
     * Cache key for a route's prerendered HTML: a hash of the compiled page (and
     * wrapper) module bytes, which — with deterministic ids — fully determine the
     * output. Returns null if the module isn't on disk (don't cache what we can't key).
     * @param {string} distPath @param {string} pathname
     */
    static async prerenderCacheKey(distPath, pathname) {
        const pageFile = Compiler.pageModuleFilePath(distPath, pathname);
        const pageBytes = await readFile(pageFile, 'utf8').catch(() => null);
        if (pageBytes == null)
            return null;
        const wrapperRoute = Compiler.resolveWrapperPage(pathname);
        const wrapperBytes = wrapperRoute
            ? await readFile(path.join(distPath, wrapperRoute.slice(1)), 'utf8').catch(() => '')
            : '';
        return Compiler.deterministicHash(`${pathname}\0${pageBytes}\0${wrapperBytes}`);
    }
    /**
     * Rewrites a route's page (and wrapper) module imports to browser-resolvable
     * paths — the side effect renderPageDocument performs before rendering, applied
     * on a cache hit where the render itself is skipped.
     * @param {string} distPath @param {string} pathname
     */
    static async rewriteModuleImportsForRoute(distPath, pathname) {
        await Compiler.rewriteAbsoluteImports(Compiler.pageModuleFilePath(distPath, pathname), distPath);
        const wrapperRoute = Compiler.resolveWrapperPage(pathname);
        if (wrapperRoute)
            await Compiler.rewriteAbsoluteImports(path.join(distPath, wrapperRoute.slice(1)), distPath);
    }
    /** @param {string} route */
    static async bundlePageFile(route) {
        route = route.replaceAll('\\', '/');
        const sourcePath = `${Router.pagesPath}/${route}`;
        const templateData = await Compiler.extractComponents(await Bun.file(sourcePath).text(), sourcePath, 'pages', route);
        const moduleRoute = Compiler.toModuleOutputRoute(route);
        const routePath = Compiler.routePathFromPageSource(route);
        const publicPath = `/pages/${moduleRoute}`;
        await Compiler.registerModule(templateData, moduleRoute, 'pages', sourcePath);
        if (templateData.hasSlot && !Compiler.wrapperPages[routePath]?.allowSelf) {
            Compiler.wrapperPages[routePath] = {
                path: publicPath,
                allowSelf: false,
            };
        }
    }
    /**
     * Registers a component's name → module path in compMapping (with duplicate
     * detection). Cheap and side-effect-free apart from the map, so it runs as a
     * serial pre-pass before the parallel compile in {@link bundleComponents}.
     * @param {string} comp
     */
    static registerComponentName(comp) {
        comp = comp.replaceAll('\\', '/');
        Compiler.validateComponentRoute(comp);
        const componentName = Compiler.normalizeComponentName(comp);
        const modulePath = comp.replace('.html', '.js');
        if (Compiler.compMapping.has(componentName) && Compiler.compMapping.get(componentName) !== modulePath) {
            throw new Error(`Duplicate component name '${componentName}' for '${comp}' and '${Compiler.compMapping.get(componentName)}'`);
        }
        Compiler.compMapping.set(componentName, modulePath);
    }
    /** @param {string} comp */
    static async bundleComponentFile(comp) {
        comp = comp.replaceAll('\\', '/');
        const sourcePath = `${Router.componentsPath}/${comp}`;
        const templateData = await Compiler.extractComponents(await Bun.file(sourcePath).text(), sourcePath, 'components', comp);
        await Compiler.registerModule(templateData, comp, 'components', sourcePath);
    }
    /** @param {string} route */
    static async bundleAssetFile(route) {
        route = route.replaceAll('\\', '/');
        const file = Bun.file(`${Router.assetsPath}/${route}`);
        const routePath = `/shared/assets/${route}`;
        Router.reqRoutes[routePath] = {
            GET: async () => typedResponse(routePath, await file.bytes(), file.type)
        };
    }
    // ── Template extraction ────────────────────────────────────────────────────
    /**
     * @param {string} sourceText
     * @param {string} sourcePath
     * @param {string} dir
     * @param {string} route
     * @returns {Promise<TemplateData>}
     */
    static async extractComponents(sourceText, sourcePath, dir, route) {
        let scriptContent = '';
        let scriptLang = 'js';
        let hasSlot = false;
        const rewriter = new HTMLRewriter()
            .on('script', {
            element(element) {
                const lang = element.getAttribute('lang');
                if (lang)
                    scriptLang = lang;
            },
            text(text) { scriptContent += text.text; }
        })
            .on('slot', {
            element() {
                hasSlot = true;
            }
        });
        let htmlContent = await rewriter.transform(new Response(sourceText)).text();
        const companion = await Compiler.getCompanionScript(sourcePath);
        const companionStylePath = await Compiler.getCompanionStylePath(sourcePath);
        if (companionStylePath) {
            const css = await readFile(companionStylePath, 'utf8');
            const companionStyle = Compiler.buildCompanionStyle(css, dir, route);
            htmlContent = `<style>${companionStyle}</style>\n${htmlContent}`;
        }
        return {
            html: htmlContent,
            hasSlot,
            script: scriptContent || undefined,
            scriptLang,
            companion: companion ?? undefined,
            companionImportPath: companion?.importPath
        };
    }
    // ── HTML → AST parsing ─────────────────────────────────────────────────────
    /**
     * @param {string} htmlContent
     * @param {string} [sourceName]
     * @param {Map<string, Set<string>>} [imports]
     * @param {string} [modulePublicPath]
     * @returns {Promise<TemplateNode[]>}
     */
    static parseHTML(htmlContent, sourceName = 'template', imports = new Map(), modulePublicPath = '') {
        return new Promise((resolve, reject) => {
            /** @type {TemplateNode[]} */
            const parsed = [];
            /** @type {string[]} */
            const tagStack = [];
            let insideScript = false;
            let insideStyle = false;
            // Element/event ids: deterministic by default (see Compiler.idGenerator),
            // so unchanged source compiles to byte-identical output — the basis of the
            // persistent prerender cache. The `\0tpl` tag keeps this id space disjoint
            // from the component-host ids minted in createJSData for the same module.
            const genHash = Compiler.idGenerator(`${sourceName ?? 'anon'}\0tpl`);
            /** @param {string} value */
            const escapeTemplateLiteral = (value) => value
                .replaceAll('\\', '\\\\')
                .replaceAll('`', '\\`')
                .replaceAll('${', '\\${');
            // Attribute values may carry raw `"` (e.g. single-quoted JSON like
            // items='[{"k":"v"}]'). We always re-serialize attributes with double
            // quotes, so any raw `"` must become `&quot;` or it closes the attribute
            // early and truncates the value in the DOM. HTMLRewriter hands us values
            // RAW (existing entities such as &amp;/&#39; are NOT decoded), so we only
            // touch `"` — re-encoding `&` would double-escape author-written entities.
            /** @param {string} value */
            const escapeAttrValue = (value) => escapeTemplateLiteral(value.replaceAll('"', '&quot;'));
            // DuVay (and other Light-DOM web components) re-interpret raw `on*`
            // attributes as native handlers, double-binding Tac's generated handler.
            // So Tac never leaves an `on:<event>` binding in the DOM: it compiles to a
            // DuVay-safe marker (`data-tac-on-<event>`) that the runtime delegation
            // keys off. The `on:` prefix (vs bare `on`) keeps real attributes/props
            // like `onboarding` from being mistaken for events. Colons in component
            // event names (`update:selected`) are encoded to `__` so they don't
            // collide with the `:binding` rewrite or HTML attr parsing.
            /** @param {string} eventName */
            const eventMarkerAttr = (eventName) => `data-tac-on-${eventName.replaceAll(':', '__')}`;
            /** @param {string} name @param {string} value @param {string} hash @param {string} tagName */
            const formatAttr = (name, value, hash, tagName) => {
                if (Compiler.nativeEventHandlerAttrs.has(name))
                    Compiler.warnBareHandler(name, tagName, sourceName);
                if (name.startsWith('on:')) {
                    const eventName = name.slice(3);
                    // Handler values must be a single expression (typically a method
                    // call). Validate at compile time so authors get a clear error
                    // instead of a raw JS parse failure surfacing from the prerender
                    // worker. Only syntax is checked; free identifiers (methods,
                    // state) resolve at render time.
                    try {
                        // eslint-disable-next-line no-new-func
                        new Function('$event', `"use strict"; return (\n${value}\n);`);
                    }
                    catch {
                        throw new Error(`Tac: the ${name} handler must be a single expression — typically a method call like ${name}="handler($event)". Multi-statement handlers or blocks are not supported. Received: ${JSON.stringify(value)}`);
                    }
                    const eventHash = genHash();
                    return `${eventMarkerAttr(eventName)}="\${await tc_invokeEvent('${eventHash}', async ($event) => { const __event__ = $event; return (${value}); }, '${hash}')}"`;
                }
                if (name === ':value' && tagName !== 'switch')
                    return `value="\${tc_escapeAttr(tc_assignValue('${hash}', '${value}', ${value}))}"`;
                if (name === ':checked')
                    return `\${${value} ? 'checked' : ''}`;
                return `${name}="${escapeAttrValue(value)}"`;
            };
            /** @param {string} text */
            const interpolate = (text) => {
                /** @type {string[]} */
                const rawExpressions = [];
                /** @type {string[]} */
                const escapedExpressions = [];
                /** @type {string[]} */
                const literalBraces = [];
                const templated = text
                    // Backslash-escaped braces are literal prose, not interpolation:
                    // `\{` → `{`, `\}` → `}`. Pulled out first so the scanners below
                    // never see them. Lets authors write `a JSON array of \{ title,
                    // value \} nodes` without entity-encoding every brace.
                    .replace(/\\([{}])/g, (_match, brace) => {
                        literalBraces.push(brace);
                        return `__TY_BRACE_${literalBraces.length - 1}__`;
                    })
                    .replace(/\{!\s*([^{}]+?)\s*\}/g, (_match, expr) => {
                    Compiler.assertValidInterpolation(expr, sourceName);
                    rawExpressions.push(expr);
                    return `__TY_RAW_${rawExpressions.length - 1}__`;
                })
                    .replace(/\{\s*([^{}!][^{}]*?)\s*\}/g, (_match, expr) => {
                    Compiler.assertValidInterpolation(expr.trim(), sourceName);
                    escapedExpressions.push(expr.trim());
                    return `__TY_EXPR_${escapedExpressions.length - 1}__`;
                });
                return escapeTemplateLiteral(templated)
                    .replace(/__TY_EXPR_(\d+)__/g, (_match, index) => {
                    const expr = escapedExpressions[Number(index)];
                    if (/^\$[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(expr)) {
                        return `<span data-tac-persist-field="${expr}">\${tc_escapeText(${expr})}</span>`;
                    }
                    return `\${tc_escapeText(${expr})}`;
                })
                    .replace(/__TY_RAW_(\d+)__/g, (_match, index) => `\${${rawExpressions[Number(index)]}}`)
                    // Restore literal braces last — `{`/`}` are inert in a template
                    // literal (only `${` and backtick are special), so this is safe.
                    .replace(/__TY_BRACE_(\d+)__/g, (_match, index) => literalBraces[Number(index)]);
            };
            const rewriter = new HTMLRewriter()
                .on('script', {
                element(el) {
                    insideScript = true;
                    el.onEndTag(() => { insideScript = false; });
                }
            })
                .on('style', {
                element(el) {
                    insideStyle = true;
                    el.onEndTag(() => { insideStyle = false; });
                },
                text(text) {
                    parsed.push({ element: `\`<style>${escapeTemplateLiteral(text.text)}</style>\`` });
                }
            })
                .on('*', {
                element(element) {
                    const tag = element.tagName.toUpperCase();
                    if (tag === 'SCRIPT' || tag === 'STYLE')
                        return;
                    if (tag === 'SLOT') {
                        parsed.push({ element: '`<div id="tc-page-slot"></div>`' });
                        element.remove();
                        return;
                    }
                    const hash = genHash();
                    const tagLower = element.tagName.toLowerCase();
                    /** @type {AttributeMap} */
                    const attrs = {};
                    for (const [name, value] of element.attributes)
                        attrs[name] = value;
                    const tagResolution = Compiler.classifyElementTag(tagLower);
                    const resolvedComponent = tagResolution.kind === 'component' ? tagResolution.name : null;
                    if (tagResolution.kind === 'unknown') {
                        Compiler.warnUnknownTag(tagLower, sourceName);
                    }
                    // Component import
                    if (resolvedComponent) {
                        const filepath = Compiler.compMapping.get(resolvedComponent);
                        const isLazy = 'lazy' in attrs;
                        if (filepath && !isLazy) {
                            const existing = imports.get(filepath);
                            if (!existing || !existing.has(resolvedComponent)) {
                                const keyword = existing ? 'const' : 'const';
                                const componentPublicPath = `/components/${filepath}`;
                                // Keep generated component routes external to
                                // Bun. Tac resolves their absolute route at
                                // runtime for web and converts it to a relative
                                // filesystem import for native bundles.
                                const importPath = componentPublicPath;
                                parsed.push({ static: `${keyword} ${resolvedComponent.replaceAll('-', '_')} = await __tc_helpers__.loadTacModule('${importPath}')` });
                                if (existing)
                                    existing.add(resolvedComponent);
                                else
                                    imports.set(filepath, new Set([resolvedComponent]));
                            }
                        }
                    }
                    // Auto-generate id for non-control, non-component elements
                    if (!attrs.id && !resolvedComponent && !Compiler.controlTags.has(tagLower)) {
                        attrs[':id'] = `tc_generateId('${hash}', 'id')`;
                    }
                    const attrStr = Object.entries(attrs).map(([n, v]) => formatAttr(n, v, hash, tagLower)).join(' ');
                    tagStack.push(tagLower);
                    if (element.selfClosing) {
                        const tagName = resolvedComponent ? `${resolvedComponent}_` : tagLower;
                        parsed.push({ element: `\`<${tagName} ${attrStr} />\`` });
                        tagStack.pop();
                    }
                    else {
                        parsed.push({ element: `\`<${tagLower} ${attrStr}>\`` });
                    }
                },
                text(text) {
                    if (text.text && !insideScript && !insideStyle) {
                        parsed.push({ element: `\`${interpolate(text.text)}\`` });
                    }
                }
            })
                .on('*', {
                element(element) {
                    if (element.selfClosing)
                        return;
                    const tag = element.tagName.toUpperCase();
                    if (tag === 'SCRIPT' || tag === 'STYLE')
                        return;
                    element.onEndTag(() => {
                        const tagName = tagStack.pop();
                        if (tagName)
                            parsed.push({ element: `\`</${tagName}>\`` });
                    });
                }
            });
            rewriter.transform(new Response(htmlContent)).text().then(() => resolve(parsed)).catch(reject);
        });
    }
    // ── JS code generation ─────────────────────────────────────────────────────
    /**
     * @param {TemplateData} templateData
     * @param {TemplateNode[]} elements
     * @returns {Promise<string>}
     */
    /** @param {TemplateData} templateData @param {TemplateNode[]} elements @param {string} publicPath */
    static async createJSData(templateData, elements, publicPath) {
        // Component-host ids: deterministic by default (see Compiler.idGenerator).
        // `\0comp` keeps this id space disjoint from parseHTML's element/event ids.
        const genHash = Compiler.idGenerator(`${publicPath}\0comp`);
        /** @type {string[]} */
        const statics = [];
        /** @type {string[]} */
        const body = [];
        for (const templateNode of elements) {
            if (templateNode.static)
                statics.push(templateNode.static);
            if (templateNode.element) {
                // Control flow and component tags are raw JS, not concatenated
                if (Compiler.isControlElementSource(templateNode.element) ||
                    /<([A-Za-z0-9-]+)_\s*([\s\S]*?)\/>/.test(templateNode.element)) {
                    body.push(templateNode.element);
                }
                else {
                    body.push(`elements+=${templateNode.element}`);
                }
            }
        }
        let renderSource = body.join('\n')
            .replaceAll(/`<loop :for="(.*?)">`|`<\/loop>`/g, (_, expr) => expr ? Compiler.compileLoopExpression(expr) : '}')
            .replaceAll(/`<logic :if="(.*?)">`/g, (_, expr) => `if(${expr}) {`)
            .replaceAll(/`<logic :else-if="(.*?)">`/g, (_, expr) => `else if(${expr}) {`)
            .replaceAll(/`<logic else(?:="")?>`|`<\/logic>`/g, (match) => match.startsWith('`<logic') ? `else {` : '}');
        Compiler.assertControlDirectivesResolved(renderSource, publicPath);
        renderSource = Compiler.compileSwitchExpressions(renderSource, publicPath);
        // Bind dynamic attributes :attr="expr" → attr="${escaped expr}"
        renderSource = renderSource.replaceAll(/:(\w[\w-]*)="([^"]*)"/g, '$1="${tc_escapeAttr($2)}"');
        // Transform component invocations
        renderSource = renderSource.replaceAll(/`<([A-Za-z0-9-]+)_\s*([\s\S]*?)\/>`/g, (_, component, attrStr) => {
            const matches = attrStr.matchAll(/([a-zA-Z0-9-@_]+)="([^"]*)"/g);
            const props = [];
            const events = [];
            const hash = genHash();
            const isLazy = /\blazy\b/.test(attrStr);
            const renderName = component.replaceAll('-', '_');
            for (const [, key, value] of matches) {
                if (key === 'lazy')
                    continue;
                // Event handlers have already been compiled to `data-tac-on-*`
                // markers by formatAttr; carry them onto the component's host div.
                if (key.startsWith('data-tac-on-')) {
                    events.push(`${key}="${value.replace(/tc_invokeEvent\('([^']+)',([\s\S]*?),\s*'([^']+)'\)/g, `tc_invokeEvent('$1',$2,'${hash}')`)}"`);
                }
                else {
                    // Component props carry the LIVE value, not a stringified
                    // attribute. A dynamic `:prop="expr"` reaches here as
                    // `${tc_escapeAttr(expr)}` (from the generic :binding rewrite
                    // above) — unwrap the escape so the raw expression is passed,
                    // letting objects/arrays through as real values. A static
                    // `prop="text"` stays a JSON string literal.
                    let expr;
                    if (value.startsWith('${') && value.endsWith('}')) {
                        const inner = value.slice(2, -1);
                        const escaped = inner.match(/^tc_escapeAttr\(([\s\S]*)\)$/);
                        expr = escaped ? escaped[1] : inner;
                    }
                    else {
                        expr = JSON.stringify(value);
                    }
                    props.push(`"${key}": ${expr}`);
                }
            }
            props.push(`"__tc_persist_id__": tc_generateId('${hash}', 'persist')`);
            const genId = "${tc_generateId('" + hash + "', 'id')}";
            const propsObj = props.length ? `{${props.join(', ')}}` : 'null';
            if (isLazy) {
                const filepath = Compiler.compMapping.get(component);
                const runtimeModulePath = Compiler.assetPath(`/components/${filepath}`);
                return `
                elements += \`<div id="${genId}" data-tac-scope="${component}" data-tac-module="${runtimeModulePath}" data-lazy-component="${component}" data-lazy-path="${runtimeModulePath}" data-lazy-props="\${${props.length ? `encodeURIComponent(JSON.stringify(${propsObj}))` : "''"}}\" ${events.join(' ')}></div>\`
                `;
            }
            const filepath = Compiler.compMapping.get(component);
            const runtimeModulePath = Compiler.assetPath(`/components/${filepath}`);
            // Hoist the host id so it can be both the div id and the key under which
            // this component's render closure is registered for scoped re-render.
            const hostVar = `__tc_host_${hash}`;
            return `
                const ${hostVar} = tc_generateId('${hash}', 'id')
                elements += \`<div id="\${${hostVar}}" data-tac-scope="${component}" data-tac-module="${runtimeModulePath}" ${events.join(' ')}>\`
                const __tc_child_props_${hash} = ${propsObj}
                const __tc_child_props_sig_${hash} = JSON.stringify(__tc_child_props_${hash})
                if(!compRenders.has('${hash}') || compRenderProps.get('${hash}') !== __tc_child_props_sig_${hash}) {
                    render = await ${renderName}(__tc_child_props_${hash})
                    compRenders.set('${hash}', render)
                    compRenderProps.set('${hash}', __tc_child_props_sig_${hash})
                } else {
                    render = compRenders.get('${hash}')
                }
                __tc_helpers__.registerComponentRender(${hostVar}, render, '${hash}')
                elements += await render(elemId, event, '${hash}')
                elements += '</div>'
            `;
        });
        // Compile-time event set: every `on:<event>` in this template became a
        // `data-tac-on-<event>` marker above. Collect the distinct event types so
        // the runtime can register one delegated document listener per type at
        // setup — no per-render DOM scan to discover handlers (see delegateEvents).
        const delegatedEventNames = [...new Set(
            [...renderSource.matchAll(/data-tac-on-([A-Za-z0-9_-]+)=/g)].map((match) => match[1].replaceAll('__', ':'))
        )];
        const rawScriptContent = await Compiler.transpileInlineScript(templateData.script ?? '', templateData.scriptLang);
        const { bindingNames: dynamicImportBindings, moduleImports: dynamicModuleImports, scriptContent, } = Compiler.liftDynamicImports(rawScriptContent);
        const moduleImports = [...dynamicModuleImports];
        const factoryBindings = [...dynamicImportBindings];
        // Add component module imports referenced by HTML template component tags
        const seenBindings = new Set(factoryBindings);
        for (const match of renderSource.matchAll(/data-tac-scope="([^"]+)"/g)) {
            const componentName = match[1];
            const filepath = Compiler.compMapping.get(componentName);
            if (!filepath) continue;
            const bindingName = componentName.replaceAll('-', '_');
            if (seenBindings.has(bindingName)) continue;
            seenBindings.add(bindingName);
            const componentPublicPath = `/components/${filepath}`;
            const importPath = componentPublicPath;
            moduleImports.push(`${bindingName}: (p) => import(${JSON.stringify(importPath)}).then(async (m) => { const f = m.default || m; return await f(p) })`);
            factoryBindings.push(bindingName);
        }
        const companionImportPath = templateData.companion?.importPath ?? templateData.companionImportPath;
        const companionLoader = companionImportPath
            ? `
const __tc_companion__ = await (async () => {
    const __tc_companion_module__ = await __tc_companion_import__();
    const __tc_Companion__ = __tc_companion_module__?.default;
    if (typeof __tc_Companion__ !== 'function') return null;
    const __tc_runtime_bindings__ = __tc_helpers__.createTacHelpers(__tc_props__);
    const __tc_instance__ = new __tc_Companion__(__tc_props__, __tc_runtime_bindings__);
    if (__tc_instance__) __tc_helpers__.bindCompanion(__tc_instance__, __tc_props__, __tc_runtime_bindings__);
    return __tc_instance__;
})();`
            : `const __tc_companion__ = null;`;
        if (companionImportPath) {
            moduleImports.push(`__tc_companion_import__: () => import(${JSON.stringify(companionImportPath)})`);
            factoryBindings.push('__tc_companion_import__');
        }
        const factorySource = `
const __tc_props__ = __tc_helpers__.decodeProps(props);
${factoryBindings.length > 0 ? `const { ${factoryBindings.join(', ')} } = __tc_module_imports__;` : ''}
${companionLoader}
const __tc_scope__ = __tc_helpers__.createScope(__tc_companion__, __tc_props__);

with (__tc_scope__) {
    const fetch = __tc_helpers__.fetch;
    const isBrowser = __tc_helpers__.isBrowser;
    const isServer = __tc_helpers__.isServer;
    const platform = __tc_helpers__.platform.platform;
    const environment = __tc_helpers__.platform.environment;
    const os = __tc_helpers__.platform.os;
    const target = __tc_helpers__.platform.target;
    const onMount = __tc_helpers__.onMount;
    const publish = __tc_helpers__.publish;
    const rerender = __tc_helpers__.rerender;
    const subscribe = __tc_helpers__.subscribe;
    const env = __tc_helpers__.env;
    const device = __tc_helpers__.device;
    const fylo = __tc_helpers__.fylo;

    ${delegatedEventNames.length ? `__tc_helpers__.delegateEvents(${JSON.stringify(delegatedEventNames)});` : ''}
    ${statics.join('\n')}
    ${scriptContent}

    if (__tc_props__) {
        for (const __k__ of Object.keys(__tc_props__)) {
            if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(__k__) && !__k__.startsWith('__tc_')) {
                const __v__ = __tc_props__[__k__];
                try { eval(\`\${__k__} = __v__\`) } catch {}
            }
        }
    }

    const compRenders = new Map();
    const compRenderProps = new Map();

    return async function(elemId, event, compId) {
        const counters = { id: {}, ev: {}, bind: {}, persist: {} };
        const tc_componentRootId = compId
            ? (String(compId).startsWith('tc-') ? String(compId) : 'tc-' + compId + '-0')
            : null;

        __tc_helpers__.setRenderContext({ componentRootId: tc_componentRootId, elemId, event });

        const tc_generateId = (hash, source, displayHash = hash) => {
            const key = compId ? hash + '-' + compId : hash;
            const map = counters[source];
            const displayKey = compId ? displayHash + '-' + compId : displayHash;

            if (key in map) {
                return 'tc-' + displayKey + '-' + map[key]++;
            }

            map[key] = 1;
            return 'tc-' + displayKey + '-0';
        };

        const tc_invokeEvent = async (hash, action, targetHash = hash) => {
            if (elemId === tc_generateId(hash, 'ev', targetHash)) {
                if (typeof action === 'function') await action(event);
                else {
                    const toCall = (event && !action.endsWith(')')) ? action + "('" + event + "')" : action;
                    await eval(toCall);
                }
            }
            return '';
        };

        const tc_assignValue = (hash, variable, currentValue) => {
            let nextValue = currentValue;
            if (elemId === tc_generateId(hash, 'bind') && event) {
                if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable)) {
                    const __val__ = event.value;
                    try { eval(\`\${variable} = __val__\`) } catch {}
                    nextValue = __val__;
                }
            }
            return nextValue ?? '';
        };

        const tc_escapeHtml = (value) => {
            if (value === null || value === undefined) return '';
            return String(value)
                .replaceAll('&', '&amp;')
                .replaceAll('<', '&lt;')
                .replaceAll('>', '&gt;')
                .replaceAll('"', '&quot;')
                .replaceAll("'", '&#39;');
        };

        const tc_escapeText = tc_escapeHtml;
        const tc_escapeAttr = tc_escapeHtml;

        let elements = '';
        let render;

        ${renderSource}

        return elements;
    };
}`;
        let code = await frameworkText('compiler/render-template.js');
        code = code.split('// module_imports').join(moduleImports.join(',\n'));
        code = code.split('"__TY_FACTORY_SOURCE__"').join(JSON.stringify(factorySource));
        code = code.split('"__TY_MODULE_PATH__"').join(JSON.stringify(publicPath));
        return code;
    }
    /** @param {string} code @param {string} publicPath */
    static wrapGlobalModule(code, publicPath) {
        const transformed = code.replace(/export default\s+async function\s*\(/, 'const __tc_default_export__ = async function(');
        return `
(function(root) {
    function ensureTac() {
        var existing = root.Tac || {};
        var modules = existing.modules || new Map();

        existing.version = existing.version || '1';
        existing.modules = modules;
        existing.register = existing.register || function(path, rendererFactory) {
            modules.set(path, rendererFactory);
            return rendererFactory;
        };
        existing.load = existing.load || async function(path) {
            var registered = modules.get(path);
            if (registered) return registered;

            var mod = await import(path);
            if (typeof mod.default === 'function') return mod.default;

            registered = modules.get(path);
            if (registered) return registered;

            throw new Error('Tac module "' + path + '" did not export or register a renderer');
        };

        root.Tac = existing;
        return existing;
    }

${transformed}
    ensureTac().register(${JSON.stringify(publicPath)}, __tc_default_export__);
})(globalThis);
`;
    }
    /**
     * @param {{ templateData: TemplateData, dir: string, route: string, sourcePath: string, publicPath: string }} options
     * @returns {BunPlugin}
     */
    static createTemplatePlugin(options) {
        const { templateData, dir, route, sourcePath, publicPath } = options;
        const filter = Compiler.createFilePathFilter(sourcePath);
        const sourceLabel = Compiler.toSourceLabel(sourcePath);
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-template:${publicPath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    const parsed = await Compiler.parseHTML(templateData.html, `${dir}/${route} (${sourceLabel})`, new Map(), publicPath);
                    const baseCode = await Compiler.createJSData(templateData, parsed, publicPath);
                    const jsCode = Compiler.isGlobalOutput()
                        ? Compiler.wrapGlobalModule(baseCode, publicPath)
                        : baseCode;
                    return {
                        contents: jsCode,
                        loader: 'js',
                    };
                });
            }
        });
    }
    /**
     * JavaScript and TypeScript keep their browser globals. Tac injects the
     * remaining portable platform names (`app`, `clipboard`, `fileSystem`,
     * `shell`, `browser`, `share`, `haptics`, `secrets`, `auth`, and `capabilities`) without exposing a framework
     * namespace in app source.
     * @param {string} source
     */
    static lowerJavaScriptNativeShims(source) {
        return source
            .replace(/\bapp\.isAvailable\(\)/g, 'this.tac.__native.app.available()')
            .replace(/\bapp\.info\(\)/g, 'this.tac.__native.app.info()')
            .replace(/\bclipboard\.writeText\(/g, 'this.tac.__native.clipboard.writeText(')
            .replace(/\bclipboard\.readText\(\)/g, 'this.tac.__native.clipboard.readText()')
            .replace(/\bfileSystem\.readText\(/g, 'this.tac.__native.fileSystem.readText(')
            .replace(/\bfileSystem\.writeText\(/g, 'this.tac.__native.fileSystem.writeText(')
            .replace(/\bfileSystem\.readDir\(/g, 'this.tac.__native.fileSystem.readDir(')
            .replace(/\bfileSystem\.paths\(\)/g, 'this.tac.__native.fileSystem.paths()')
            .replace(/\bshell\.exec\(/g, 'this.tac.__native.shell.exec(')
            .replace(/\bbrowser\.open\(/g, 'this.tac.__native.browser.open(')
            .replace(/\bshare\.text\(/g, 'this.tac.__native.share.text(')
            .replace(/\bhaptics\.impact\(\)/g, 'this.tac.__native.haptics.impact()')
            .replace(/\bfilePicker\.openText\(\)/g, 'this.tac.__native.filePicker.openText()')
            .replace(/\bfilePicker\.saveText\(/g, 'this.tac.__native.filePicker.saveText(')
            .replace(/\bsecrets\.get\(/g, 'this.tac.__native.secrets.get(')
            .replace(/\bsecrets\.set\(/g, 'this.tac.__native.secrets.set(')
            .replace(/\bsecrets\.delete\(/g, 'this.tac.__native.secrets.delete(')
            .replace(/\bauth\.verifyUser\(/g, 'this.tac.__native.auth.verifyUser(')
            .replace(/\bgeolocation\.current\(/g, 'this.tac.__native.geolocation.current(')
            .replace(/\bnotifications\.show\(/g, 'this.tac.__native.notifications.show(')
            .replace(/\bmedia\.getUserMedia\(/g, 'this.tac.__native.media.getUserMedia(')
            .replace(/\bhost\.on\(/g, 'this.tac.__native.host.on(')
            .replace(/\bcapabilities\.supports\(/g, 'this.tac.__native.capabilities.supports(')
            .replace(/\bcapabilities\.state\(/g, 'this.tac.__native.capabilities.state(')
    }
    /** @param {string} source @param {string} sourcePath */
    static assertNoLegacyJavaScriptPlatformWrappers(source, sourcePath) {
        if (/\b(?:App|Browser|FilePicker)\./.test(Compiler.maskCommentsAndStrings(source)))
            throw new Error(`Tac companion '${Compiler.toSourceLabel(sourcePath)}' uses a removed platform wrapper. Use app, browser, or filePicker from the implicit prelude instead.`);
    }
    /**
     * @param {string} sourcePath
     * @returns {BunPlugin}
     */
    static createCompanionScriptPlugin(sourcePath) {
        const filter = Compiler.createFilePathFilter(sourcePath);
        const tacInline = `
const __tc_noopPlatform__ = Object.freeze({ target: 'web', platform: 'web', environment: 'web', os: 'web', browserOS: 'unknown', native: false, web: true, desktop: false, mobile: false });
const __tc_nativeUnavailable__ = async (operation) => { throw new Error(\`Native shim operation '\${operation}' is unavailable outside a bundled Tac app\`) };
const __tc_noopHelpers__ = {
    isBrowser: false, isServer: true, bindPersistentFields: () => {}, env: (_key, fallback) => fallback,
    platform: __tc_noopPlatform__, props: {}, fetch: (input, init) => fetch(input, init),
    __native: {
        capabilities: { supports: () => false, state: async () => 'unsupported' },
        app: { available: () => false, info: () => __tc_nativeUnavailable__('app.info') },
        clipboard: { available: () => false, readText: () => __tc_nativeUnavailable__('clipboard.readText'), writeText: () => __tc_nativeUnavailable__('clipboard.writeText') },
        fileSystem: { readText: () => __tc_nativeUnavailable__('fs.readText'), writeText: () => __tc_nativeUnavailable__('fs.writeText'), readDir: () => __tc_nativeUnavailable__('fs.readDir'), paths: () => __tc_nativeUnavailable__('fs.paths') },
        shell: { exec: () => __tc_nativeUnavailable__('shell.exec') },
        browser: { available: () => false, open: () => __tc_nativeUnavailable__('browser.open') },
        share: { text: () => __tc_nativeUnavailable__('share.text') },
        haptics: { impact: () => __tc_nativeUnavailable__('haptics.impact') },
        filePicker: { available: () => false, openText: () => __tc_nativeUnavailable__('filePicker.openText'), saveText: () => __tc_nativeUnavailable__('filePicker.saveText') },
        secrets: { get: () => __tc_nativeUnavailable__('secrets.get'), set: () => __tc_nativeUnavailable__('secrets.set'), delete: () => __tc_nativeUnavailable__('secrets.delete') },
        auth: { verifyUser: () => __tc_nativeUnavailable__('auth.verifyUser') },
        geolocation: { current: () => __tc_nativeUnavailable__('geo.current') },
        notifications: { show: () => __tc_nativeUnavailable__('notify.show') },
        media: { getUserMedia: () => __tc_nativeUnavailable__('media.getUserMedia') },
        host: { on: () => () => {} },
        web: {
            localStorage: { getItem: (_key, fallback) => fallback, setItem: () => {}, removeItem: () => {} },
            sessionStorage: { getItem: (_key, fallback) => fallback, setItem: () => {}, removeItem: () => {} },
            navigator: { language: () => '', online: () => false, userAgent: () => '' },
            location: { href: () => '', origin: () => '' }, fetch: (input, init) => fetch(input, init),
        },
    },
    __nativeCall: __tc_nativeUnavailable__, onMount: () => {}, publish: () => false, rerender: () => {},
    subscribe: (_name, callbackOrFallback) => typeof callbackOrFallback === 'function' ? () => {} : callbackOrFallback,
};
class Tac { props; tac; constructor(props = {}, tac = __tc_noopHelpers__) { this.props = props; this.tac = tac; } }
`;
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-companion:${sourcePath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    let contents = await Bun.file(sourcePath).text();
                    Compiler.assertNoRemovedDecorators(contents, sourcePath);
                    Compiler.assertNoLegacyJavaScriptPlatformWrappers(contents, sourcePath);
                    const decoratorNames = Compiler.findReferencedDecorators(contents);
                    if (decoratorNames.length > 0) {
                        const decoratorsEntryPath = await frameworkFilePath('runtime/decorators.js');
                        const importPath = Compiler.toRelativeImportPath(sourcePath, decoratorsEntryPath);
                        contents = `import { ${decoratorNames.join(', ')} } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    if (Compiler.referencesFyloGlobal(contents)) {
                        const fyloGlobalEntryPath = await frameworkFilePath('runtime/fylo-global.js');
                        const importPath = Compiler.toRelativeImportPath(sourcePath, fyloGlobalEntryPath);
                        contents = `import { fylo } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    contents = Compiler.lowerJavaScriptNativeShims(contents);
                    contents = Compiler.injectTacBaseClass(contents);
                    if (Compiler.shouldInjectTacImport(contents)) {
                        contents = `${tacInline}\n${contents}`;
                    }
                    return {
                        contents,
                        loader: Compiler.loaderForFilePath(sourcePath),
                    };
                });
            }
        });
    }
    /**
     * Compiles a Dart companion into the same controller constructor shape as
     * JavaScript and TypeScript companions. The compiler content-addresses
     * its output, so repeat loads are cache hits unless the source or Dart
     * version changed — no per-plugin memo that could serve stale output.
     * @param {string} sourcePath
     * @returns {BunPlugin}
     */
    static createDartCompanionPlugin(sourcePath) {
        const filter = Compiler.createFilePathFilter(sourcePath);
        const compiler = new DartCompanionCompiler();
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-dart-companion:${sourcePath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    const source = await Bun.file(sourcePath).text();
                    const compiled = await compiler.compile(sourcePath);
                    const runtimeRoute = Compiler.dartCompanionRuntimeRoute(sourcePath);
                    Router.reqRoutes[runtimeRoute] = {
                        GET: () => jsResponse(runtimeRoute, compiled.runtimeCode),
                    };
                    let contents = compiler.createJavaScriptAdapter(compiled.contract, compiled.factoryName, './tac.dart.js');
                    if (Compiler.referencesFyloFacade(source)) {
                        const fyloGlobalEntryPath = await frameworkFilePath('runtime/fylo-global.js');
                        const importPath = Compiler.toRelativeImportPath(sourcePath, fyloGlobalEntryPath);
                        contents = `import ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    return {
                        contents,
                        loader: 'js',
                    };
                });
            },
        });
    }
    /**
     * Compiles a language-native Tac subset into the same controller ABI used
     * by JavaScript, TypeScript, and Dart companions.
     * @param {string} sourcePath
     * @param {TacSubsetLanguage} language
     * @returns {BunPlugin}
     */
    static createSubsetCompanionPlugin(sourcePath, language) {
        const filter = Compiler.createFilePathFilter(sourcePath);
        const compiler = new TacSubsetCompanionCompiler(language);
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-${language}-companion:${sourcePath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    const source = await Bun.file(sourcePath).text();
                    let contents = compiler.compile(source, sourcePath).code;
                    if (Compiler.referencesFyloFacade(source)) {
                        const fyloGlobalEntryPath = await frameworkFilePath('runtime/fylo-global.js');
                        const importPath = Compiler.toRelativeImportPath(sourcePath, fyloGlobalEntryPath);
                        contents = `import ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    return {
                        contents,
                        loader: 'js',
                    };
                });
            },
        });
    }
    /** @param {string} sourcePath */
    static dartCompanionRuntimeRoute(sourcePath) {
        const candidates = [
            [Router.pagesPath, 'pages'],
            [Router.componentsPath, 'components'],
        ];
        for (const [root, kind] of candidates) {
            const relative = path.relative(root, sourcePath);
            if (!relative.startsWith('..') && !path.isAbsolute(relative))
                return `/${kind}/${relative.replaceAll(path.sep, '/')}.js`;
        }
        throw new Error(`Dart Tac companion '${sourcePath}' must live under client/pages or client/components.`);
    }
    /**
     * @param {TemplateData} templateData
     * @param {string} sourcePath
     * @returns {BunPlugin[]}
     */
    static createCompanionPlugins(templateData, sourcePath) {
        if (templateData.companion) {
            if (templateData.companion.provider.target === 'ecmascript')
                return [Compiler.createCompanionScriptPlugin(templateData.companion.sourcePath)];
            if (templateData.companion.provider.target === 'dart')
                return [Compiler.createDartCompanionPlugin(templateData.companion.sourcePath)];
            if (templateData.companion.provider.target === 'subset')
                return [Compiler.createSubsetCompanionPlugin(
                    templateData.companion.sourcePath,
                    /** @type {TacSubsetLanguage} */ (templateData.companion.provider.language),
                )];
        }
        if (templateData.companionImportPath) {
            const companionPath = path.resolve(path.dirname(sourcePath), templateData.companionImportPath);
            return [Compiler.createCompanionScriptPlugin(companionPath)];
        }
        return [];
    }
    // ── Build & register a single template module ──────────────────────────────
    /**
     * @param {TemplateData} templateData
     * @param {string} route
     * @param {string} dir
     * @param {string} sourcePath
     */
    static async registerModule(templateData, route, dir, sourcePath) {
        const publicPath = `/${dir}/${Compiler.toModuleOutputRoute(route)}`;
        if (Compiler.isGlobalOutput()) {
            const parsed = await Compiler.parseHTML(templateData.html, `${dir}/${route} (${Compiler.toSourceLabel(sourcePath)})`, new Map(), publicPath);
            const baseCode = await Compiler.createJSData(templateData, parsed, publicPath);
            const output = new Bun.Transpiler({ loader: 'js' })
                .transformSync(Compiler.wrapGlobalModule(baseCode, publicPath));
            Router.reqRoutes[publicPath] = {
                GET: () => jsResponse(publicPath, output)
            };
            return;
        }
        const result = await Bun.build({
            entrypoints: [sourcePath],
            external: ['/components/*', `${MODULES_ROUTE_PREFIX}/*`],
            minify: { whitespace: true, syntax: true },
            splitting: false,
            plugins: [
                ...Compiler.createCompanionPlugins(templateData, sourcePath),
                Compiler.createTemplatePlugin({
                    templateData,
                    dir,
                    route,
                    sourcePath,
                    publicPath,
                })
            ],
        });
        Router.reqRoutes[publicPath] = {
            GET: () => jsResponse(publicPath, result.outputs[0])
        };
    }
    // ── Asset bundlers ─────────────────────────────────────────────────────────
    static async bundleAssets() {
        if (!await pathExists(Router.assetsPath))
            return;
        for (const route of new Bun.Glob('**/*').scanSync({ cwd: Router.assetsPath })) {
            const file = Bun.file(`${Router.assetsPath}/${route}`);
            const routePath = `/shared/assets/${route}`;
            Router.reqRoutes[routePath] = {
                GET: async () => typedResponse(routePath, await file.bytes(), file.type)
            };
        }
    }
    static async bundleSharedData() {
        if (!await pathExists(Router.sharedDataPath))
            return;
        for (const route of new Bun.Glob('**/*').scanSync({ cwd: Router.sharedDataPath })) {
            const file = Bun.file(`${Router.sharedDataPath}/${route}`);
            const routePath = `/shared/data/${route}`;
            Router.reqRoutes[routePath] = {
                GET: async () => typedResponse(routePath, await file.bytes(), file.type)
            };
        }
    }
    static async bundlePages() {
        if (await pathExists(Router.pagesPath)) {
            const routes = Array.from(new Bun.Glob(`**/${Compiler.pageFileName}`).scanSync({ cwd: Router.pagesPath }));
            if (await Bun.file(path.join(Router.pagesPath, Compiler.pageFileName)).exists() && !routes.includes(Compiler.pageFileName))
                routes.unshift(Compiler.pageFileName);
            // Pages compile independently: each reads the now-stable compMapping
            // and writes a distinct wrapperPages key, so they parallelize safely.
            let pageIndex = 0;
            const pageWorkers = Array.from({ length: Math.min(Compiler.compileConcurrency, routes.length) }, async () => {
                while (pageIndex < routes.length) {
                    await Compiler.bundlePageFile(routes[pageIndex++]);
                }
            });
            await Promise.all(pageWorkers);
        }
        // 404 page
        const nfFile = Bun.file(`${process.cwd()}/404.html`);
        const nfContent = await nfFile.exists()
            ? await nfFile.text()
            : await frameworkText('runtime/shells/not-found.html');
        const nfSourcePath = await nfFile.exists()
            ? `${process.cwd()}/404.html`
            : await frameworkFilePath('runtime/shells/not-found.html');
        const nfData = await Compiler.extractComponents(nfContent, nfSourcePath, 'pages', '404.html');
        await Compiler.registerModule(nfData, '404.html', 'pages', nfSourcePath);
    }
    static async bundleComponents() {
        if (!await pathExists(Router.componentsPath))
            return;
        const comps = Array.from(new Bun.Glob(`**/${Router.pageFileName}`).scanSync({ cwd: Router.componentsPath }));
        // Phase 1 (serial, cheap): populate compMapping for every component so
        // sibling-component references resolve during the parallel compile.
        for (const comp of comps)
            Compiler.registerComponentName(comp);
        // Phase 2 (parallel): the heavy extract + Bun.build per component.
        let compIndex = 0;
        const compWorkers = Array.from({ length: Math.min(Compiler.compileConcurrency, comps.length) }, async () => {
            while (compIndex < comps.length) {
                await Compiler.bundleComponentFile(comps[compIndex++]);
            }
        });
        await Promise.all(compWorkers);
    }
    /**
     * @param {string} specifier
     * @returns {string}
     */
    static packageNameFromSpecifier(specifier) {
        if (!specifier || specifier.startsWith('.') || specifier.startsWith('/') || specifier.includes('://'))
            return '';
        const parts = specifier.split('/');
        if (specifier.startsWith('@'))
            return parts.length >= 2 ? `${parts[0]}/${parts[1]}` : '';
        return parts[0] ?? '';
    }
    /**
     * @param {string} source
     * @returns {string[]}
     */
    static importedPackageNames(source) {
        const names = new Set();
        const patterns = [
            /\bimport\s+(?:[^'"]*?\s+from\s+)?['"]([^'"]+)['"]/g,
            /\bexport\s+[^'"]*?\s+from\s+['"]([^'"]+)['"]/g,
            /\bimport\s*\(\s*['"]([^'"]+)['"]\s*\)/g,
        ];
        for (const pattern of patterns) {
            for (const match of source.matchAll(pattern)) {
                const name = Compiler.packageNameFromSpecifier(String(match[1] ?? ''));
                if (name)
                    names.add(name);
            }
        }
        return [...names];
    }
    /**
     * @returns {Promise<Set<string>>}
     */
    static async collectBrowserDependencyNames() {
        const sourceRoots = [
            Router.pagesPath,
            Router.componentsPath,
            Router.sharedScriptsPath,
        ];
        const dependencies = new Set();
        for (const root of sourceRoots) {
            if (!await pathExists(root))
                continue;
            for (const route of new Bun.Glob('**/*.{js,ts,mjs,mts}').scanSync({ cwd: root })) {
                const source = await Bun.file(path.join(root, route)).text();
                for (const name of Compiler.importedPackageNames(source))
                    dependencies.add(name);
            }
        }
        return dependencies;
    }
    /**
     * Loads a user-provided import map from the shared directory.
     * Prefers `client/shared/importmap.json`, then falls back to
     * `shared/importmap.json`. Returns an empty object if neither exists.
     * @returns {Promise<Record<string, string>>}
     */
    static async loadUserImportMap() {
        const candidates = [
            path.join(Router.sharedScriptsPath, '..', 'importmap.json'),
            path.join(process.cwd(), 'shared', 'importmap.json'),
        ];
        for (const candidate of candidates) {
            const file = Bun.file(candidate);
            if (!await file.exists())
                continue;
            try {
                const map = await file.json();
                if (map && typeof map.imports === 'object' && map.imports !== null)
                    return /** @type {Record<string, string>} */ (map.imports);
                Compiler.compilerLogger.warn('Import map file is missing a valid "imports" object', { path: candidate });
                return {};
            }
            catch (error) {
                Compiler.compilerLogger.error('Failed to parse import map file', { path: candidate, err: error });
                throw new Error(`Invalid import map: ${candidate}`);
            }
        }
        return {};
    }
    /**
     * Generates the inline import map for the current app. Combines
     * auto-generated entries for bundled npm dependencies with user-defined
     * entries from `client/shared/importmap.json`. User entries take
     * precedence. Paths are adjusted for native bundles.
     * @returns {Promise<Record<string, string>>}
     */
    static async generateImportMap() {
        const imports = await Compiler.loadUserImportMap();
        const packageFile = Bun.file(`${process.cwd()}/package.json`);
        const dependencies = await (async () => {
            if (Compiler.bundledDependencyModules.size > 0)
                return Compiler.bundledDependencyModules;
            if (!await packageFile.exists())
                return new Set();
            const packages = await packageFile.json();
            const browserDependencies = await Compiler.collectBrowserDependencyNames();
            return new Set(Object.keys(packages.dependencies ?? {}).filter((moduleName) => browserDependencies.has(moduleName)));
        })();
        for (const moduleName of dependencies) {
            if (Object.prototype.hasOwnProperty.call(imports, moduleName))
                continue;
            imports[moduleName] = Compiler.assetPath(Compiler.moduleRoutePath(moduleName));
        }
        return imports;
    }
    static async bundleDependencies() {
        const packageFile = Bun.file(`${process.cwd()}/package.json`);
        if (!await packageFile.exists())
            return;
        const packages = await packageFile.json();
        const browserDependencies = await Compiler.collectBrowserDependencyNames();
        const modules = Object.keys(packages.dependencies ?? {}).filter((moduleName) => browserDependencies.has(moduleName));
        Compiler.bundledDependencyModules = new Set();
        for (const moduleName of modules) {
            // Scoped packages use a + separator in route paths so the /
            // in @scope/name is not treated as a path segment delimiter.
            const routeKey = Compiler.moduleRouteKey(moduleName);
            // Resolve to a file path so Bun.build handles scoped packages
            // without trying to open @scope as a root directory.
            let entry = moduleName;
            try {
                const resolved = Bun.resolveSync(moduleName, process.cwd());
                if (resolved && resolved !== moduleName)
                    entry = resolved;
            }
            catch {
                // Fall through — Bun.build may still resolve the bare specifier
            }
            try {
                const result = await Bun.build({
                    entrypoints: [entry],
                    minify: true
                });
                const routePath = Compiler.moduleRoutePath(moduleName);
                for (const output of result.outputs) {
                    Router.reqRoutes[routePath] = {
                        GET: () => jsResponse(routePath, output)
                    };
                }
                Compiler.bundledDependencyModules.add(moduleName);
            }
            catch (error) {
                Compiler.compilerLogger.warn('Failed to bundle dependency module', { module: moduleName, err: error });
            }
        }
    }
}
