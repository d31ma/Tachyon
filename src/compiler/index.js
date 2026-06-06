// @ts-check
import Router from "../server/http/route-handler.js";
import { createPublicBrowserEnvResponse, PUBLIC_BROWSER_ENV_PATH, withPublicBrowserEnv } from "../server/http/browser-env.js";
import logger from "../server/observability/logger.js";
import { existsSync } from 'fs';
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from "path";
import { pathToFileURL } from "url";
import { compileRustWorker } from "./wasm/rust-compiler.js";
import { compileCWorker } from "./wasm/c-compiler.js";
import { compileCppWorker } from "./wasm/cpp-compiler.js";
import { compilePythonWorker } from "./wasm/python-compiler.js";
import { compileCSharpWorker } from "./wasm/csharp-compiler.js";
import { compileGoWorker } from "./wasm/go-compiler.js";
import { compileZigWorker } from "./wasm/zig-compiler.js";
import { compileJavaScriptWorker, compileTypeScriptWorker } from "./wasm/javascript-compiler.js";
/**
 * @typedef {import("bun").BunRequest} BunRequest
 * @typedef {import("bun").BunPlugin} BunPlugin
 * @typedef {{ path: string, allowSelf: boolean }} LayoutEntry
 * @typedef {Record<string, LayoutEntry>} LayoutMap
 * @typedef {'esm' | 'global'} OutputFormat
 * @typedef {'ecmascript'} CompanionTarget
 * @typedef {'rust' | 'c' | 'cpp' | 'zig' | 'python' | 'csharp' | 'go' | 'javascript' | 'typescript'} TacWorkerLanguage
 * @typedef {{ extension: string, target: 'ecmascript' }} CompanionProvider
 * @typedef {{ sourcePath: string, importPath: string, provider: CompanionProvider }} CompanionScript
 * @typedef {{ extension: string, language: TacWorkerLanguage }} TacWorkerProvider
 * @typedef {{ sourcePath: string, route: string, provider: TacWorkerProvider }} TacWorkerScript
 * @typedef {{ html: string, hasSlot: boolean, script?: string, scriptLang: string, companion?: CompanionScript, companionImportPath?: string }} TemplateData
 * @typedef {{ static?: string, element?: string }} TemplateNode
 * @typedef {Record<string, string>} AttributeMap
 * @typedef {{ version: string, modules: Map<string, Function>, register: (id: string, factory: Function) => Function, load: (id: string) => Promise<Function> }} TacRegistry
 */
const TEMPLATE_PATH = `${import.meta.dir}/render-template.js`;
const SPA_RENDERER_PATH = `${import.meta.dir}/../runtime/spa-renderer.js`;
const FYLO_LOCAL_WORKER_PATH = `${import.meta.dir}/../runtime/fylo-local-worker.js`;
const ROUTE_MANIFEST_PLACEHOLDER = '{"__tachyonPlaceholder":true}';
const LAYOUT_MANIFEST_PLACEHOLDER = '{"__tachyonShellPlaceholder":true}';
const NOT_FOUND_PATH = `${import.meta.dir}/../runtime/shells/not-found.html`;
const APP_SHELL_PATH = `${import.meta.dir}/../runtime/shells/app.html`;
const PRERENDER_WORKER_PATH = `${import.meta.dir}/../runtime/prerender-worker.js`;
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
    /** @type {OutputFormat} */
    static outputFormat = Compiler.resolveOutputFormat();
    /** @type {string[]} */
    static mainEntryCandidates = ['imports.ts', 'imports.js'];
    /** @type {CompanionProvider[]} */
    static companionProviders = [
        { extension: '.js', target: 'ecmascript' },
        { extension: '.ts', target: 'ecmascript' },
    ];
    /** @type {TacWorkerProvider[]} */
    static workerProviders = [
        { extension: '.rs', language: 'rust' },
        { extension: '.c', language: 'c' },
        { extension: '.cpp', language: 'cpp' },
        { extension: '.zig', language: 'zig' },
        { extension: '.py', language: 'python' },
        { extension: '.cs', language: 'csharp' },
        { extension: '.go', language: 'go' },
        { extension: '.js', language: 'javascript' },
        { extension: '.ts', language: 'typescript' },
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
    /** @type {LayoutMap} */
    static layoutMapping = {};
    /** @type {Set<string>} */
    static warnedUnknownTags = new Set();
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
            `Example: browser/components/user/card/${Router.pageFileName} is used as <user-card />.`,
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
                    valueVar: `__ty_switch_value_${id}`,
                    matchedVar: `__ty_switch_matched_${id}`,
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
                return `{ const __ty_case_value=(${caseExpr}); if(!${frame.matchedVar} && __ty_helpers__.matchSwitchCase(${frame.valueVar}, __ty_case_value)) { ${frame.matchedVar}=true;`;
            }
            if (match === '`<case default="">`') {
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
    static async createStaticRoutes() {
        Compiler.compMapping.clear();
        Compiler.layoutMapping = {};
        Compiler.warnedUnknownTags.clear();
        await Router.validatePageRoutes();
        // Components must be registered before pages/layouts compile so that
        // compMapping is fully populated when import statements are generated.
        await Compiler.bundleComponents();
        await Compiler.bundlePages();
        await Compiler.bundleWorkers();
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
    static resolveLayout(pathname) {
        if (pathname !== '/') {
            const segments = pathname.split('/').filter(Boolean);
            for (let count = segments.length; count > 0; count -= 1) {
                const prefix = `/${segments.slice(0, count).join('/')}`;
                const entry = Compiler.layoutMapping[prefix];
                if (entry && (entry.allowSelf || prefix !== pathname))
                    return entry.path;
            }
        }
        const rootEntry = Compiler.layoutMapping['/'];
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
        const rewritten = source.replace(/import\("(?<spec>\/(?:components|modules)\/[^"]+)"\)/g, (_match, spec) => {
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
            : path.join(cwd, 'browser', 'shared', 'scripts');
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
            ? '/pages/tac.js'
            : `/pages${Router.routeToFilesystemPath(pathname)}/tac.js`;
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
    static shouldInjectTacImport(source) {
        const stripped = source
            .replace(/\/\*[\s\S]*?\*\//g, '')
            .replace(/\/\/.*$/gm, '')
            .replace(/(['"`])(?:\\.|(?!\1).)*\1/g, '');
        return !(/^\s*import\s+(?:type\s+)?(?:\{[^}]*\bTac\b[^}]*\}|\bTac\b)/m.test(stripped)
            || /^\s*(?:const|let|var|class|function)\s+Tac\b/m.test(stripped));
    }
    /**
     * Returns the subset of supported decorator names that the source uses as
     * decorations (`@<name>` outside of comments and string literals) and that
     * are not already imported or locally declared.
     * @param {string} source
     * @returns {string[]}
     */
    static findReferencedDecorators(source) {
        const stripped = source
            .replace(/\/\*[\s\S]*?\*\//g, '')
            .replace(/\/\/.*$/gm, '')
            .replace(/(['"`])(?:\\.|(?!\1).)*\1/g, '');
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
        const stripped = source
            .replace(/\/\*[\s\S]*?\*\//g, '')
            .replace(/\/\/.*$/gm, '')
            .replace(/(['"`])(?:\\.|(?!\1).)*\1/g, '');
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
        const stripped = source
            .replace(/\/\*[\s\S]*?\*\//g, '')
            .replace(/\/\/.*$/gm, '')
            .replace(/(['"`])(?:\\.|(?!\1).)*\1/g, '');
        const usagePattern = /(?:^|[^.\w@])fylo\b/m;
        if (!usagePattern.test(stripped)) return false;
        const importPattern = /^\s*import\s+[\s\S]*?\bfylo\b[\s\S]*?\bfrom\b/m;
        const declarationPattern = /^\s*(?:const|let|var|class|function)\s+fylo\b/m;
        if (importPattern.test(stripped) || declarationPattern.test(stripped)) return false;
        return true;
    }
    /**
     * Detect companions that call `fetch('tac://...')` so the build can inject
     * the Tac-aware `fetch` shadow (which routes `tac://` URLs to Tac Workers
     * and delegates everything else to the platform fetch).
     * @param {string} source
     */
    static referencesTacFetch(source) {
        if (!source.includes('tac://'))
            return false;
        const stripped = source
            .replace(/\/\*[\s\S]*?\*\//g, '')
            .replace(/\/\/.*$/gm, '');
        if (!/\bfetch\s*\(/.test(stripped))
            return false;
        const importPattern = /^\s*import\s+[\s\S]*?\bfetch\b[\s\S]*?\bfrom\b/m;
        const declarationPattern = /^\s*(?:const|let|var|function)\s+fetch\b/m;
        return !(importPattern.test(stripped) || declarationPattern.test(stripped));
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
            const helperName = `__ty_dynamic_import_${index++}__`;
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
    /** @param {string} shellHTML */
    static withMainStylesheet(shellHTML) {
        if (!Router.reqRoutes['/imports.css'] || shellHTML.includes('/imports.css'))
            return shellHTML;
        return shellHTML.replace('</head>', '    <link rel="stylesheet" href="/imports.css">\n</head>');
    }
    /**
     * @param {{ includeHotReloadClient?: boolean }} [options]
     * @returns {Promise<string>}
     */
    static async renderShellHTML(options = {}) {
        const includeHotReloadClient = options.includeHotReloadClient === true;
        const fyloBrowserPath = process.env.YON_DATA_BROWSER_PATH || '/_fylo';
        let shellHTML = await Bun.file(APP_SHELL_PATH).text();
        shellHTML = shellHTML
            .replace('<!--__TACHYON_DEV_HEAD__-->', includeHotReloadClient
                ? '    <script type="module" src="/hot-reload-client.js"></script>'
                : '')
            .replace('__FYLO_BROWSER_PATH__', fyloBrowserPath);
        return withPublicBrowserEnv(Compiler.withMainStylesheet(shellHTML));
    }
    static createSpaRendererManifestPlugin() {
        const routeManifestJSON = JSON.stringify(Router.routeSlugs);
        const layoutManifestJSON = JSON.stringify(Compiler.layoutMapping);
        const escapedRouteManifestJSON = routeManifestJSON
            .replaceAll('\\', '\\\\')
            .replaceAll("'", "\\'");
        const escapedLayoutManifestJSON = layoutManifestJSON
            .replaceAll('\\', '\\\\')
            .replaceAll("'", "\\'");
        return /** @type {BunPlugin} */ ({
            name: 'tachyon-manifest-inline',
            setup(build) {
                build.onLoad({ filter: /spa-renderer\.js$/ }, async ({ path: filePath }) => {
                    if (path.resolve(filePath) !== path.resolve(SPA_RENDERER_PATH))
                        return undefined;
                    const source = await Bun.file(filePath).text();
                    if (!source.includes(ROUTE_MANIFEST_PLACEHOLDER))
                        throw new Error('Tac SPA renderer route manifest placeholder is missing');
                    if (!source.includes(LAYOUT_MANIFEST_PLACEHOLDER))
                        throw new Error('Tac SPA renderer layout manifest placeholder is missing');
                    return {
                        contents: source
                            .replace(ROUTE_MANIFEST_PLACEHOLDER, escapedRouteManifestJSON)
                            .replace(LAYOUT_MANIFEST_PLACEHOLDER, escapedLayoutManifestJSON),
                        loader: 'js',
                    };
                });
            },
        });
    }
    static async bundleBrowserRuntimeAssets() {
        const entrypoints = [
            SPA_RENDERER_PATH,
            FYLO_LOCAL_WORKER_PATH,
            `${import.meta.dir}/../runtime/hot-reload-client.js`,
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
            plugins: [Compiler.createSpaRendererManifestPlugin()],
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
    /**
     * @param {string} distPath
     * @returns {{ title: string, restore: () => void }}
     */
    static installPrerenderGlobals(distPath) {
        const previousDocument = globalThis.document;
        const previousWindow = globalThis.window;
        const previousTac = /** @type {{ Tac?: TacRegistry }} */ (globalThis).Tac;
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
            __ty_prerender__: true,
            Tac: /** @type {TacRegistry} */ ({
                version: '1',
                modules,
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
                Reflect.deleteProperty(globalThis, '__ty_prerender__');
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
        const layoutRoute = Compiler.resolveLayout(pathname);
        const layoutFile = layoutRoute
            ? path.join(distPath, layoutRoute.slice(1))
            : null;
        if (layoutFile)
            await Compiler.rewriteAbsoluteImports(layoutFile, distPath);
        const prerender = Compiler.installPrerenderGlobals(distPath);
        try {
            let layoutHTML = '';
            if (layoutFile) {
                const layoutModule = await Compiler.loadRenderFactoryForPrerender(layoutFile, /** @type {string} */ (layoutRoute));
                const layoutFactory = await layoutModule();
                layoutHTML = await layoutFactory();
            }
            const pageModule = await Compiler.loadRenderFactoryForPrerender(pageFile, pagePublicPath);
            const pageFactory = await pageModule();
            const pageHTML = await pageFactory();
            const bodyHTML = layoutHTML
                ? layoutHTML.replace('<div id="ty-layout-slot"></div>', () => `<div id="ty-layout-slot">${pageHTML}</div>`)
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
    /** @param {string} distPath @param {string} pathname @param {string} shellHTML @param {LayoutMap} layoutMapping */
    static async renderPageDocumentForWorker(distPath, pathname, shellHTML, layoutMapping) {
        const previousLayoutMapping = Compiler.layoutMapping;
        Compiler.layoutMapping = { ...layoutMapping };
        try {
            return await Compiler.renderPageDocument(distPath, pathname, shellHTML);
        }
        finally {
            Compiler.layoutMapping = previousLayoutMapping;
        }
    }
    /** @param {string} distPath @param {string} pathname @param {string} shellHTML */
    static async renderPageDocumentIsolated(distPath, pathname, shellHTML) {
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
            layoutMapping: Compiler.layoutMapping,
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
        /** @type {{ outputFile: string, html: string }[]} */
        const renderedRoutes = [];
        let renderIndex = 0;
        const renderWorkers = Array.from({ length: Math.min(Compiler.prerenderRenderConcurrency, routes.length) }, async () => {
            while (renderIndex < routes.length) {
                const currentIndex = renderIndex++;
                const route = routes[currentIndex];
                const outputFile = route === '/'
                    ? path.join(distPath, 'index.html')
                    : path.join(distPath, Router.routeToFilesystemPath(route).slice(1), 'index.html');
                const html = routes.length === 1
                    ? await Compiler.renderPageDocument(distPath, route, shellHTML)
                    : await Compiler.renderPageDocumentIsolated(distPath, route, shellHTML);
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
    /** @param {string} route */
    static async bundlePageFile(route) {
        route = route.replaceAll('\\', '/');
        const sourcePath = `${Router.pagesPath}/${route}`;
        const templateData = await Compiler.extractComponents(await Bun.file(sourcePath).text(), sourcePath, 'pages', route);
        const moduleRoute = Compiler.toModuleOutputRoute(route);
        const routePath = Compiler.routePathFromPageSource(route);
        const publicPath = `/pages/${moduleRoute}`;
        await Compiler.registerModule(templateData, moduleRoute, 'pages', sourcePath);
        if (templateData.hasSlot && !Compiler.layoutMapping[routePath]?.allowSelf) {
            Compiler.layoutMapping[routePath] = {
                path: publicPath,
                allowSelf: false,
            };
        }
    }
    /** @param {string} comp */
    static async bundleComponentFile(comp) {
        comp = comp.replaceAll('\\', '/');
        Compiler.validateComponentRoute(comp);
        const componentName = Compiler.normalizeComponentName(comp);
        const modulePath = comp.replace('.html', '.js');
        const sourcePath = `${Router.componentsPath}/${comp}`;
        if (Compiler.compMapping.has(componentName) && Compiler.compMapping.get(componentName) !== modulePath) {
            throw new Error(`Duplicate component name '${componentName}' for '${comp}' and '${Compiler.compMapping.get(componentName)}'`);
        }
        Compiler.compMapping.set(componentName, modulePath);
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
     * @returns {Promise<TemplateNode[]>}
     */
    static parseHTML(htmlContent, sourceName = 'template', imports = new Map()) {
        return new Promise((resolve) => {
            /** @type {TemplateNode[]} */
            const parsed = [];
            /** @type {string[]} */
            const tagStack = [];
            let insideScript = false;
            let insideStyle = false;
            const genHash = () => Bun.randomUUIDv7().replace(/-/g, '').slice(-8);
            /** @param {string} value */
            const escapeTemplateLiteral = (value) => value
                .replaceAll('\\', '\\\\')
                .replaceAll('`', '\\`')
                .replaceAll('${', '\\${');
            /** @param {string} name @param {string} value @param {string} hash @param {string} tagName */
            const formatAttr = (name, value, hash, tagName) => {
                if (name.startsWith('@')) {
                    const eventHash = genHash();
                    return `${name}="\${await ty_invokeEvent('${eventHash}', async ($event) => { const __event__ = $event; return ${value} }, '${hash}')}"`;
                }
                if (name === ':value' && tagName !== 'switch')
                    return `value="\${ty_escapeAttr(ty_assignValue('${hash}', '${value}', ${value}))}"`;
                if (name === ':checked')
                    return `\${${value} ? 'checked' : ''}`;
                return `${name}="${escapeTemplateLiteral(value)}"`;
            };
            /** @param {string} text */
            const interpolate = (text) => {
                /** @type {string[]} */
                const rawExpressions = [];
                /** @type {string[]} */
                const escapedExpressions = [];
                const templated = text
                    .replace(/\{!\s*([^{}]+?)\s*\}/g, (_match, expr) => {
                    rawExpressions.push(expr);
                    return `__TY_RAW_${rawExpressions.length - 1}__`;
                })
                    .replace(/\{\s*([^{}!][^{}]*?)\s*\}/g, (_match, expr) => {
                    escapedExpressions.push(expr.trim());
                    return `__TY_EXPR_${escapedExpressions.length - 1}__`;
                });
                return escapeTemplateLiteral(templated)
                    .replace(/__TY_EXPR_(\d+)__/g, (_match, index) => {
                    const expr = escapedExpressions[Number(index)];
                    if (/^\$[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(expr)) {
                        return `<span data-tac-persist-field="${expr}">\${ty_escapeText(${expr})}</span>`;
                    }
                    return `\${ty_escapeText(${expr})}`;
                })
                    .replace(/__TY_RAW_(\d+)__/g, (_match, index) => `\${${rawExpressions[Number(index)]}}`);
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
                        parsed.push({ element: '`<div id="ty-layout-slot"></div>`' });
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
                                parsed.push({ static: `${keyword} ${resolvedComponent.replaceAll('-', '_')} = await __ty_helpers__.loadTacModule('/components/${filepath}')` });
                                if (existing)
                                    existing.add(resolvedComponent);
                                else
                                    imports.set(filepath, new Set([resolvedComponent]));
                            }
                        }
                    }
                    // Auto-generate id for non-control, non-component elements
                    if (!attrs.id && !resolvedComponent && !Compiler.controlTags.has(tagLower)) {
                        attrs[':id'] = `ty_generateId('${hash}', 'id')`;
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
            rewriter.transform(new Response(htmlContent)).text().then(() => resolve(parsed));
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
            .replaceAll(/(`<logic else="">`)|(`<\/logic>`)/g, (_, expr) => expr ? `else {` : '}');
        renderSource = Compiler.compileSwitchExpressions(renderSource, publicPath);
        // Bind dynamic attributes :attr="expr" → attr="${escaped expr}"
        renderSource = renderSource.replaceAll(/:(\w[\w-]*)="([^"]*)"/g, '$1="${ty_escapeAttr($2)}"');
        // Transform component invocations
        renderSource = renderSource.replaceAll(/`<([A-Za-z0-9-]+)_\s*([\s\S]*?)\/>`/g, (_, component, attrStr) => {
            const matches = attrStr.matchAll(/([a-zA-Z0-9-@]+)="([^"]*)"/g);
            const props = [];
            const events = [];
            const hash = genHash();
            const isLazy = /\blazy\b/.test(attrStr);
            const renderName = component.replaceAll('-', '_');
            for (const [, key, value] of matches) {
                if (key === 'lazy')
                    continue;
                if (key.startsWith('@')) {
                    events.push(`${key}="${value.replace(/ty_invokeEvent\('([^']+)',([\s\S]*?),\s*'([^']+)'\)/g, `ty_invokeEvent('$1',$2,'${hash}')`)}"`);
                }
                else {
                    // Convert template-literal expr "${foo()}" → foo(), static value → JSON literal
                    const expr = value.startsWith('${') && value.endsWith('}')
                        ? value.slice(2, -1)
                        : JSON.stringify(value);
                    props.push(`"${key}": ${expr}`);
                }
            }
            props.push(`"__ty_persist_id__": ty_generateId('${hash}', 'persist')`);
            const genId = "${ty_generateId('" + hash + "', 'id')}";
            const propsObj = props.length ? `{${props.join(', ')}}` : 'null';
            if (isLazy) {
                const filepath = Compiler.compMapping.get(component);
                return `
                elements += \`<div id="${genId}" data-tac-scope="${component}" data-tac-module="/components/${filepath}" data-lazy-component="${component}" data-lazy-path="/components/${filepath}" data-lazy-props="\${${props.length ? `encodeURIComponent(JSON.stringify(${propsObj}))` : "''"}}\" ${events.join(' ')}></div>\`
                `;
            }
            const filepath = Compiler.compMapping.get(component);
            return `
                elements += \`<div id="${genId}" data-tac-scope="${component}" data-tac-module="/components/${filepath}" ${events.join(' ')}>\`
                const __ty_child_props_${hash} = ${propsObj}
                const __ty_child_props_sig_${hash} = JSON.stringify(__ty_child_props_${hash})
                if(!compRenders.has('${hash}') || compRenderProps.get('${hash}') !== __ty_child_props_sig_${hash}) {
                    render = await ${renderName}(__ty_child_props_${hash})
                    elements += await render(elemId, event, '${hash}')
                    compRenders.set('${hash}', render)
                    compRenderProps.set('${hash}', __ty_child_props_sig_${hash})
                } else {
                    render = compRenders.get('${hash}')
                    elements += await render(elemId, event, '${hash}')
                }
                elements += '</div>'
            `;
        });
        const rawScriptContent = await Compiler.transpileInlineScript(templateData.script ?? '', templateData.scriptLang);
        const { bindingNames: dynamicImportBindings, moduleImports: dynamicModuleImports, scriptContent, } = Compiler.liftDynamicImports(rawScriptContent);
        const moduleImports = [...dynamicModuleImports];
        const factoryBindings = [...dynamicImportBindings];
        // Add component module imports referenced by HTML template component tags
        const seenBindings = new Set(factoryBindings);
        for (const match of renderSource.matchAll(/data-tac-module="\/components\/([^"]+)"/g)) {
            const componentPath = match[1];
            const componentName = Compiler.normalizeComponentName(componentPath.replace(/\.js$/, '.html'));
            const bindingName = componentName.replaceAll('-', '_');
            if (!seenBindings.has(bindingName)) {
                seenBindings.add(bindingName);
                moduleImports.push(`${bindingName}: (p) => import(${JSON.stringify(`/components/${componentPath}`)}).then(async (m) => { const f = m.default || m; return await f(p) })`);
                factoryBindings.push(bindingName);
            }
        }
        const companionImportPath = templateData.companion?.importPath ?? templateData.companionImportPath;
        const companionLoader = companionImportPath
            ? `
const __ty_companion__ = await (async () => {
    const __ty_companion_module__ = await __ty_companion_import__();
    const __ty_Companion__ = __ty_companion_module__?.default;
    if (typeof __ty_Companion__ !== 'function') return null;
    const __ty_runtime_bindings__ = __ty_helpers__.createTacHelpers(__ty_props__);
    const __ty_instance__ = new __ty_Companion__(__ty_props__, __ty_runtime_bindings__);
    if (__ty_instance__) __ty_helpers__.bindCompanion(__ty_instance__, __ty_props__, __ty_runtime_bindings__);
    return __ty_instance__;
})();`
            : `const __ty_companion__ = null;`;
        if (companionImportPath) {
            moduleImports.push(`__ty_companion_import__: () => import(${JSON.stringify(companionImportPath)})`);
            factoryBindings.push('__ty_companion_import__');
        }
        const factorySource = `
const __ty_props__ = __ty_helpers__.decodeProps(props);
${factoryBindings.length > 0 ? `const { ${factoryBindings.join(', ')} } = __ty_module_imports__;` : ''}
${companionLoader}
const __ty_scope__ = __ty_helpers__.createScope(__ty_companion__, __ty_props__);

with (__ty_scope__) {
    const fetch = __ty_helpers__.fetch;
    const isBrowser = __ty_helpers__.isBrowser;
    const isServer = __ty_helpers__.isServer;
    const onMount = __ty_helpers__.onMount;
    const publish = __ty_helpers__.publish;
    const rerender = __ty_helpers__.rerender;
    const subscribe = __ty_helpers__.subscribe;
    const env = __ty_helpers__.env;
    const fylo = __ty_helpers__.fylo;

    ${statics.join('\n')}
    ${scriptContent}

    if (__ty_props__) {
        for (const __k__ of Object.keys(__ty_props__)) {
            if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(__k__) && !__k__.startsWith('__ty_')) {
                const __v__ = __ty_props__[__k__];
                try { eval(\`\${__k__} = __v__\`) } catch {}
            }
        }
    }

    const compRenders = new Map();
    const compRenderProps = new Map();

    return async function(elemId, event, compId) {
        const counters = { id: {}, ev: {}, bind: {}, persist: {} };
        const ty_componentRootId = compId
            ? (String(compId).startsWith('ty-') ? String(compId) : 'ty-' + compId + '-0')
            : null;

        __ty_helpers__.setRenderContext({ componentRootId: ty_componentRootId, elemId, event });

        const ty_generateId = (hash, source, displayHash = hash) => {
            const key = compId ? hash + '-' + compId : hash;
            const map = counters[source];
            const displayKey = compId ? displayHash + '-' + compId : displayHash;

            if (key in map) {
                return 'ty-' + displayKey + '-' + map[key]++;
            }

            map[key] = 1;
            return 'ty-' + displayKey + '-0';
        };

        const ty_invokeEvent = async (hash, action, targetHash = hash) => {
            if (elemId === ty_generateId(hash, 'ev', targetHash)) {
                if (typeof action === 'function') await action(event);
                else {
                    const toCall = (event && !action.endsWith(')')) ? action + "('" + event + "')" : action;
                    await eval(toCall);
                }
            }
            return '';
        };

        const ty_assignValue = (hash, variable, currentValue) => {
            let nextValue = currentValue;
            if (elemId === ty_generateId(hash, 'bind') && event) {
                if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable)) {
                    const __val__ = event.value;
                    try { eval(\`\${variable} = __val__\`) } catch {}
                    nextValue = __val__;
                }
            }
            return nextValue ?? '';
        };

        const ty_escapeHtml = (value) => {
            if (value === null || value === undefined) return '';
            return String(value)
                .replaceAll('&', '&amp;')
                .replaceAll('<', '&lt;')
                .replaceAll('>', '&gt;')
                .replaceAll('"', '&quot;')
                .replaceAll("'", '&#39;');
        };

        const ty_escapeText = ty_escapeHtml;
        const ty_escapeAttr = ty_escapeHtml;

        let elements = '';
        let render;

        ${renderSource}

        return elements;
    };
}`;
        let code = await Bun.file(TEMPLATE_PATH).text();
        code = code.split('// module_imports').join(moduleImports.join(',\n'));
        code = code.split('"__TY_FACTORY_SOURCE__"').join(JSON.stringify(factorySource));
        code = code.split('"__TY_MODULE_PATH__"').join(JSON.stringify(publicPath));
        return code;
    }
    /** @param {string} code @param {string} publicPath */
    static wrapGlobalModule(code, publicPath) {
        const transformed = code.replace(/export default\s+async function\s*\(/, 'const __ty_default_export__ = async function(');
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
    ensureTac().register(${JSON.stringify(publicPath)}, __ty_default_export__);
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
                    const parsed = await Compiler.parseHTML(templateData.html, `${dir}/${route} (${sourceLabel})`);
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
     * @param {string} sourcePath
     * @returns {BunPlugin}
     */
    static createCompanionScriptPlugin(sourcePath) {
        const filter = Compiler.createFilePathFilter(sourcePath);
        const tacInline = `var __ty_noopHelpers__={isBrowser:!1,isServer:!0,bindPersistentFields:()=>{},env:(_,f)=>f,props:{},fetch:(i,n)=>fetch(i,n),onMount:()=>{},publish:()=>!1,rerender:()=>{},subscribe:(_,c)=>typeof c=="function"?()=>{}:c};class Tac{props;tac;constructor(props={},tac=__ty_noopHelpers__){this.props=props,this.tac=tac}}`;
        const decoratorsEntryPath = `${import.meta.dir}/../runtime/decorators.js`;
        const fyloGlobalEntryPath = `${import.meta.dir}/../runtime/fylo-global.js`;
        const tacWorkerEntryPath = `${import.meta.dir}/../runtime/tac-worker.js`;
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-companion:${sourcePath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    let contents = await Bun.file(sourcePath).text();
                    Compiler.assertNoRemovedDecorators(contents, sourcePath);
                    const decoratorNames = Compiler.findReferencedDecorators(contents);
                    if (decoratorNames.length > 0) {
                        const importPath = Compiler.toRelativeImportPath(sourcePath, decoratorsEntryPath);
                        contents = `import { ${decoratorNames.join(', ')} } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    if (Compiler.referencesFyloGlobal(contents)) {
                        const importPath = Compiler.toRelativeImportPath(sourcePath, fyloGlobalEntryPath);
                        contents = `import { fylo } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    if (Compiler.referencesTacFetch(contents)) {
                        const importPath = Compiler.toRelativeImportPath(sourcePath, tacWorkerEntryPath);
                        contents = `import { fetch } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
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
     * @param {TemplateData} templateData
     * @param {string} sourcePath
     * @returns {BunPlugin[]}
     */
    static createCompanionPlugins(templateData, sourcePath) {
        if (templateData.companion) {
            if (templateData.companion.provider.target === 'ecmascript')
                return [Compiler.createCompanionScriptPlugin(templateData.companion.sourcePath)];
        }
        if (templateData.companionImportPath) {
            const companionPath = path.resolve(path.dirname(sourcePath), templateData.companionImportPath);
            return [Compiler.createCompanionScriptPlugin(companionPath)];
        }
        return [];
    }
    /** @param {string} route */
    static validateWorkerRoute(route) {
        const normalized = route.replaceAll('\\', '/');
        const segments = normalized.split('/');
        if (segments.length < 2 || segments.at(-1)?.startsWith('tac.') !== true) {
            throw new Error(`Invalid Tac worker path '${route}'. Workers must live under browser/workers/**/tac.<language>.`);
        }
        const validSegment = /^[a-z][a-z0-9-]*$/;
        if (!segments.slice(0, -1).every((segment) => validSegment.test(segment))) {
            throw new Error(`Invalid Tac worker path '${route}'. Worker route folders must be lowercase alphanumeric or hyphenated.`);
        }
    }

    /** @param {string} route */
    static workerRouteName(route) {
        const normalized = route.replaceAll('\\', '/');
        const parts = normalized.split('/');
        parts.pop();
        return parts.join('/');
    }

    /** @param {string} sourcePath */
    static getWorkerProvider(sourcePath) {
        const extension = path.extname(sourcePath).toLowerCase();
        return Compiler.workerProviders.find((provider) => provider.extension === extension) ?? null;
    }

    /**
     * In-house Tac→Wasm frontends keyed by language. Each lowers handler-shaped
     * source (a `class Handler` with HTTP-verb methods, within the supported
     * subset) to worker-ABI wasm with no external toolchain (no rustc/clang/emcc).
     * Shared by the browser worker pipeline and the server `wasm-compiled`
     * execution backend.
     * @type {Partial<Record<string, (source: string) => Uint8Array>>}
     */
    static subsetFrontends = {
        rust: compileRustWorker,
        c: compileCWorker,
        cpp: compileCppWorker,
        zig: compileZigWorker,
        python: compilePythonWorker,
        csharp: compileCSharpWorker,
        go: compileGoWorker,
        javascript: compileJavaScriptWorker,
        typescript: compileTypeScriptWorker,
    };

    /** @returns {string[]} Languages that have an in-house subset frontend. */
    static subsetLanguages() {
        return Object.keys(Compiler.subsetFrontends);
    }

    /**
     * Compile handler-shaped source to worker-ABI wasm with the in-house
     * compiler. Throws if the language has no frontend or the source exceeds the
     * supported subset (callers may treat a throw as "fall back to subprocess").
     * @param {string} language
     * @param {string} source
     * @returns {Uint8Array}
     */
    static compileSubsetHandlerSource(language, source) {
        const compile = Compiler.subsetFrontends[language];
        if (!compile)
            throw new Error(`No in-house frontend for '${language}'; supported: ${Compiler.subsetLanguages().join(', ')}.`);
        return compile(source);
    }

    /**
     * Compile a Tac Worker source file to wasm using Tachyon's in-house
     * compiler - no external toolchain (rustc/clang/emcc) required.
     * @param {TacWorkerScript} worker
     * @returns {Promise<Uint8Array>}
     */
    static async compileWorkerSource(worker) {
        const compile = Compiler.subsetFrontends[worker.provider.language];
        if (!compile) {
            throw new Error([
                `Tac worker '${Compiler.toSourceLabel(worker.sourcePath)}' uses '${worker.provider.language}',`,
                `but the in-house Tac Worker compiler currently supports: ${Compiler.subsetLanguages().join(', ')}.`,
                `The route is reserved for a future ${worker.provider.language} frontend.`,
            ].join(' '));
        }
        const source = await readFile(worker.sourcePath, 'utf8');
        try {
            return compile(source);
        }
        catch (error) {
            const detail = error instanceof Error ? error.message : String(error);
            throw new Error(`Failed to compile Tac worker '${Compiler.toSourceLabel(worker.sourcePath)}': ${detail}`);
        }
    }

    /** @param {string} route @param {unknown} [schema] */
    static createWorkerRuntimeSource(route, schema = null) {
        const sourceLabel = JSON.stringify(route);
        const schemaSource = JSON.stringify(schema);
        return `const __tac_worker_source__ = ${sourceLabel};
const __tac_worker_schema__ = ${schemaSource};
const __tac_text_encoder__ = new TextEncoder();
const __tac_text_decoder__ = new TextDecoder();

function __tac_is_plain_object__(value) {
    return value && typeof value === 'object' && !Array.isArray(value);
}

	function __tac_schema_key_is_regex__(key) {
	    return key.startsWith('^');
	}

	function __tac_has_value__(value) {
	    return value !== null && value !== undefined;
	}

	function __tac_schema_data_key__(key) {
	    return key.endsWith('?') ? key.slice(0, -1) : key;
	}

	function __tac_schema_key_is_nullable__(key) {
	    return key.endsWith('?');
	}

	function __tac_is_record_schema__(schema) {
	    const keys = Object.keys(schema);
	    return keys.length === 1 && __tac_schema_key_is_regex__(keys[0]);
	}

	function __tac_regex__(pattern, path) {
	    if (typeof pattern !== 'string' || pattern.length === 0) {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' schema value at ' + path + ' must be a non-empty CHEX regex string');
	    }
	    if (pattern.length > 500) {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' schema regex at ' + path + ' exceeds 500 characters');
	    }
	    try {
	        return new RegExp(pattern);
	    }
	    catch {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' schema regex at ' + path + ' is invalid');
	    }
	}

	function __tac_validate_leaf__(value, pattern, path) {
	    if (!__tac_regex__(pattern, path).test(String(value))) {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' schema mismatch at ' + path);
	    }
	}

	function __tac_validate_value__(value, schema, path) {
	    if (typeof schema === 'string') {
	        __tac_validate_leaf__(value, schema, path);
	        return;
	    }
	    if (Array.isArray(schema)) {
	        if (schema.length !== 1) {
	            throw new Error('Tac worker ' + __tac_worker_source__ + ' array schema at ' + path + ' must contain exactly one item template');
	        }
	        if (!Array.isArray(value)) {
	            throw new Error('Tac worker ' + __tac_worker_source__ + ' expected an array at ' + path);
	        }
	        for (let i = 0; i < value.length; i += 1) __tac_validate_value__(value[i], schema[0], path + '[' + i + ']');
	        return;
	    }
	    if (__tac_is_plain_object__(schema)) {
	        __tac_validate_object__(value, schema, path);
	        return;
	    }
	    throw new Error('Tac worker ' + __tac_worker_source__ + ' schema value at ' + path + ' must be a CHEX regex string, array, or object');
	}

	function __tac_validate_record__(value, schema, path) {
	    if (!__tac_is_plain_object__(value)) {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' expected an object at ' + path);
	    }
	    const keyPattern = Object.keys(schema)[0];
	    const valuePattern = schema[keyPattern];
	    if (typeof valuePattern !== 'string') {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' record schema at ' + path + ' must use a regex string value');
	    }
	    for (const [key, childValue] of Object.entries(value)) {
	        __tac_validate_leaf__(key, keyPattern, path + '.<key:' + key + '>');
	        __tac_validate_leaf__(childValue, valuePattern, path + '.' + key);
	    }
	}

	function __tac_validate_object__(value, schema, path) {
	    if (__tac_is_record_schema__(schema)) {
	        __tac_validate_record__(value, schema, path);
	        return;
	    }
	    if (!__tac_is_plain_object__(schema) || Object.keys(schema).length === 0) {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' schema at ' + path + ' must define at least one property');
	    }
	    if (!__tac_is_plain_object__(value)) {
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' expected an object at ' + path);
	    }
	    for (const dataKey of Object.keys(value)) {
	        if (Object.prototype.hasOwnProperty.call(schema, dataKey) || Object.prototype.hasOwnProperty.call(schema, dataKey + '?')) continue;
	        throw new Error('Tac worker ' + __tac_worker_source__ + ' unknown property at ' + path + '.' + dataKey);
	    }
	    for (const [rawKey, childSchema] of Object.entries(schema)) {
	        const key = __tac_schema_data_key__(rawKey);
	        const childPath = path + '.' + key;
	        const childValue = value[key];
	        if (!__tac_has_value__(childValue) && __tac_schema_key_is_nullable__(rawKey)) continue;
	        if (!__tac_has_value__(childValue)) {
	            throw new Error('Tac worker ' + __tac_worker_source__ + ' missing required key at ' + childPath);
	        }
	        __tac_validate_value__(childValue, childSchema, childPath);
	    }
	}

function __tac_declared_request_sections__(request, schema) {
    const subset = {};
    for (const rawKey of Object.keys(schema || {})) {
        const key = rawKey.endsWith('?') ? rawKey.slice(0, -1) : rawKey;
        if (Object.prototype.hasOwnProperty.call(request, key)) subset[key] = request[key];
    }
    return subset;
}

function __tac_validate_request__(method, request) {
    const methodSchema = __tac_worker_schema__ && __tac_worker_schema__[method];
    const requestSchema = methodSchema && methodSchema.request;
    if (!requestSchema) return;
    __tac_validate_value__(__tac_declared_request_sections__(request, requestSchema), requestSchema, 'request');
}

function __tac_validate_response__(method, response) {
    const methodSchema = __tac_worker_schema__ && __tac_worker_schema__[method];
    if (!methodSchema) return;
    const status = String(response && response.status ? response.status : 200);
    const responseSchema = methodSchema[status] || (methodSchema.response && methodSchema.response[status]);
    if (!responseSchema) return;
    __tac_validate_value__(response && response.body !== undefined ? response.body : response, responseSchema, 'response.' + status);
}

class TacWorkerRuntime {
    constructor() {
        this.exports = null;
        this.ready = this.instantiate();
    }

    async instantiate() {
        let wasmMemory;
        const memoryView = () => {
            if (!(wasmMemory instanceof WebAssembly.Memory)) {
                throw new Error('Tac worker ' + __tac_worker_source__ + ' memory is not available yet');
            }
            return new Uint8Array(wasmMemory.buffer);
        };
        const imports = {
            env: {
                abort(_messagePtr, _filePtr, line, column) {
                    throw new Error('Tac worker ' + __tac_worker_source__ + ' aborted at ' + line + ':' + column);
                },
                memcpy(dest, src, len) {
                    memoryView().copyWithin(dest, src, src + len);
                    return dest;
                },
                memmove(dest, src, len) {
                    memoryView().copyWithin(dest, src, src + len);
                    return dest;
                },
                memset(dest, value, len) {
                    memoryView().fill(value & 255, dest, dest + len);
                    return dest;
                },
            },
            wasi_snapshot_preview1: {
                args_get() { return 0; },
                args_sizes_get() { return 0; },
                clock_time_get() { return 0; },
                environ_get() { return 0; },
                environ_sizes_get() { return 0; },
                fd_close() { return 0; },
                fd_fdstat_get() { return 0; },
                fd_seek() { return 0; },
                fd_write() { return 0; },
                poll_oneoff() { return 0; },
                proc_exit(code) {
                    throw new Error('Tac worker ' + __tac_worker_source__ + ' exited with code ' + code);
                },
                random_get() { return 0; },
            },
        };
        const wasmUrl = new URL('./tac.wasm', import.meta.url);
        const response = await fetch(wasmUrl);
        let result;
        if (WebAssembly.instantiateStreaming && response.headers.get('Content-Type')?.includes('application/wasm')) {
            result = await WebAssembly.instantiateStreaming(response, imports);
        }
        else {
            result = await WebAssembly.instantiate(await response.arrayBuffer(), imports);
        }
        const instance = result.instance ?? result;
        this.exports = instance.exports;
        if (!(this.exports.memory instanceof WebAssembly.Memory)) {
            throw new Error('Tac worker ' + __tac_worker_source__ + ' must export memory');
        }
        wasmMemory = this.exports.memory;
        for (const name of ['alloc', 'call', 'output_ptr', 'output_len']) {
            if (typeof this.exports[name] !== 'function') {
                throw new Error('Tac worker ' + __tac_worker_source__ + ' must export ' + name + '()');
            }
        }
    }

    requireExports() {
        if (!this.exports) throw new Error('Tac worker ' + __tac_worker_source__ + ' has not finished loading');
        return this.exports;
    }

    writeJson(value) {
        return this.writeText(JSON.stringify(value ?? null));
    }

    writeText(text) {
        const exports = this.requireExports();
        const bytes = __tac_text_encoder__.encode(String(text));
        const ptr = exports.alloc(bytes.length || 1);
        if (!ptr) throw new Error('Tac worker ' + __tac_worker_source__ + ' failed to allocate ' + bytes.length + ' bytes');
        new Uint8Array(exports.memory.buffer).set(bytes, ptr);
        return { ptr, len: bytes.length };
    }

    dealloc(input) {
        const exports = this.requireExports();
        if (input && typeof exports.dealloc === 'function') exports.dealloc(input.ptr, input.len);
    }

    readOutput() {
        const exports = this.requireExports();
        const ptr = exports.output_ptr();
        const len = exports.output_len();
        if (!len) return {};
        const memory = new Uint8Array(exports.memory.buffer);
        const text = __tac_text_decoder__.decode(memory.subarray(ptr, ptr + len));
        try {
            return JSON.parse(text);
        }
        catch (error) {
            throw new Error('Tac worker ' + __tac_worker_source__ + ' produced invalid JSON output: ' + text.slice(0, 200));
        }
    }

    call(method, request) {
        const exports = this.requireExports();
        __tac_validate_request__(method, request ?? {});
        const rawBody = request && typeof request === 'object' ? request.body : undefined;
        const bodyText = typeof rawBody === 'string' ? rawBody : JSON.stringify(rawBody ?? null);
        const methodInput = this.writeJson(method);
        const requestInput = this.writeJson(request);
        const bodyInput = this.writeText(bodyText);
        try {
            exports.call(methodInput.ptr, methodInput.len, requestInput.ptr, requestInput.len, bodyInput.ptr, bodyInput.len);
            const response = this.readOutput();
            __tac_validate_response__(method, response);
            return response;
        }
        finally {
            this.dealloc(methodInput);
            this.dealloc(requestInput);
            this.dealloc(bodyInput);
        }
    }
}

const runtime = new TacWorkerRuntime();

self.onmessage = async (event) => {
    const { id, method, request } = event.data ?? {};
    try {
        await runtime.ready;
        const response = runtime.call(String(method || ''), request ?? {});
        self.postMessage({ id, ok: true, response });
    }
    catch (error) {
        self.postMessage({
            id,
            ok: false,
            error: error instanceof Error ? error.message : String(error),
        });
    }
};
`;
    }

    /** @param {string} route */
    static async bundleWorkerFile(route) {
        route = route.replaceAll('\\', '/');
        Compiler.validateWorkerRoute(route);
        const sourcePath = path.join(Router.workersPath, route);
        const provider = Compiler.getWorkerProvider(sourcePath);
        if (!provider)
            return;
        const workerRoute = Compiler.workerRouteName(route);
        /** @type {TacWorkerScript} */
        const worker = { sourcePath, route: workerRoute, provider };
        const wasmBytes = await Compiler.compileWorkerSource(worker);
        const publicBase = `/workers/${workerRoute}`;
        const wasmRoute = `${publicBase}/tac.wasm`;
        const workerScriptRoute = `${publicBase}/tac.worker.js`;
        const schemaPath = path.join(path.dirname(sourcePath), Router.optionsFileName);
        const schema = await pathExists(schemaPath)
            ? JSON.parse(await readFile(schemaPath, 'utf8'))
            : null;
        const workerScript = Compiler.createWorkerRuntimeSource(workerRoute, schema);
        Router.reqRoutes[wasmRoute] = {
            GET: () => staticRouteResponse(wasmRoute, wasmBytes, 'application/wasm')
        };
        Router.reqRoutes[workerScriptRoute] = {
            GET: () => jsResponse(workerScriptRoute, workerScript)
        };
        if (schema) {
            const optionsRoute = `${publicBase}/${Router.optionsFileName}`;
            Router.reqRoutes[optionsRoute] = {
                GET: () => staticRouteResponse(optionsRoute, JSON.stringify(schema, null, 2), 'application/json')
            };
        }
    }

    static async bundleWorkers() {
        if (!await pathExists(Router.workersPath))
            return;
        const extensions = new Set(Compiler.workerProviders.map((provider) => provider.extension));
        for (const route of new Bun.Glob('**/tac.*').scanSync({ cwd: Router.workersPath })) {
            if (!extensions.has(path.extname(route).toLowerCase()))
                continue;
            await Compiler.bundleWorkerFile(route);
        }
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
            const parsed = await Compiler.parseHTML(templateData.html, `${dir}/${route} (${Compiler.toSourceLabel(sourcePath)})`);
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
            external: ['/components/*', '/modules/*'],
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
            for (const route of routes) {
                await Compiler.bundlePageFile(route);
            }
        }
        // 404 page
        const nfFile = Bun.file(`${process.cwd()}/404.html`);
        const nfContent = await nfFile.exists()
            ? await nfFile.text()
            : await Bun.file(NOT_FOUND_PATH).text();
        const nfSourcePath = await nfFile.exists() ? `${process.cwd()}/404.html` : NOT_FOUND_PATH;
        const nfData = await Compiler.extractComponents(nfContent, nfSourcePath, 'pages', '404.html');
        await Compiler.registerModule(nfData, '404.html', 'pages', nfSourcePath);
    }
    static async bundleComponents() {
        if (!await pathExists(Router.componentsPath))
            return;
        for (const comp of new Bun.Glob(`**/${Router.pageFileName}`).scanSync({ cwd: Router.componentsPath })) {
            await Compiler.bundleComponentFile(comp);
        }
    }
    static async bundleDependencies() {
        const packageFile = Bun.file(`${process.cwd()}/package.json`);
        if (!await packageFile.exists())
            return;
        const packages = await packageFile.json();
        const modules = Object.keys(packages.dependencies ?? {});
        for (const moduleName of modules) {
            // Scoped packages use a + separator in route paths so the /
            // in @scope/name is not treated as a path segment delimiter.
            const routeKey = moduleName.replace('/', '+');
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
                for (const output of result.outputs) {
                    Router.reqRoutes[`/modules/${routeKey}.js`] = {
                        GET: () => jsResponse(`/modules/${routeKey}.js`, output)
                    };
                }
            }
            catch (error) {
                Compiler.compilerLogger.warn('Failed to bundle dependency module', { module: moduleName, err: error });
            }
        }
    }
}
function genHash() {
    return Bun.randomUUIDv7().replace(/-/g, '').slice(-8);
}
