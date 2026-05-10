// @ts-check
import Router from "../server/http/route-handler.js";
import { createPublicBrowserEnvResponse, PUBLIC_BROWSER_ENV_PATH, withPublicBrowserEnv } from "../server/http/browser-env.js";
import logger from "../server/observability/logger.js";
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import { existsSync } from "fs";
import { tmpdir } from 'os';
import path from "path";
import { pathToFileURL } from "url";
/**
 * @typedef {import("bun").BunRequest} BunRequest
 * @typedef {import("bun").BunPlugin} BunPlugin
 * @typedef {{ path: string, allowSelf: boolean }} LayoutEntry
 * @typedef {Record<string, LayoutEntry>} LayoutMap
 * @typedef {'esm' | 'global'} OutputFormat
 * @typedef {'ecmascript' | 'wasm-json' | 'wasm-source'} CompanionTarget
 * @typedef {'wat' | 'assemblyscript' | 'rust' | 'c' | 'go' | 'zig'} WasmSourceLanguage
 * @typedef {{ extension: string, target: 'ecmascript' | 'wasm-json' } | { extension: string, target: 'wasm-source', language: WasmSourceLanguage }} CompanionProvider
 * @typedef {{ sourcePath: string, importPath: string, provider: CompanionProvider }} CompanionScript
 * @typedef {{ abi: 'tac-wasm-json@1', methods?: string[], state?: Record<string, unknown> }} TacWasmManifest
 * @typedef {{ html: string, hasSlot: boolean, script?: string, scriptLang: string, companion?: CompanionScript, companionImportPath?: string }} TemplateData
 * @typedef {{ static?: string, element?: string }} TemplateNode
 * @typedef {Record<string, string>} AttributeMap
 * @typedef {{ version: string, modules: Map<string, Function>, register: (id: string, factory: Function) => Function, load: (id: string) => Promise<Function> }} TacRegistry
 */
const TEMPLATE_PATH = `${import.meta.dir}/render-template.js`;
const ROUTES_JSON_PATH = `${import.meta.dir}/../shared/manifests/route-manifest.json`;
const SHELLS_JSON_PATH = `${import.meta.dir}/../shared/manifests/shell-manifest.json`;
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
/** @param {string} routePath @param {string} filePath */
const jsonResponse = (routePath, filePath) => async () => staticRouteResponse(routePath, await Bun.file(filePath).bytes(), 'application/json');
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
        { extension: '.as.ts', target: 'wasm-source', language: 'assemblyscript' },
        { extension: '.rs', target: 'wasm-source', language: 'rust' },
        { extension: '.c', target: 'wasm-source', language: 'c' },
        { extension: '.go', target: 'wasm-source', language: 'go' },
        { extension: '.zig', target: 'wasm-source', language: 'zig' },
        { extension: '.wat', target: 'wasm-source', language: 'wat' },
        { extension: '.wasm', target: 'wasm-json' },
    ];
    /** @type {string[]} */
    static companionScriptExtensions = Compiler.companionProviders.map((provider) => provider.extension);
    /** Names of decorators auto-imported into companion scripts when referenced as `@<name>`. */
    static companionDecoratorNames = ['inject', 'provide', 'env', 'onMount', 'emit', 'render'];
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
    static controlTags = new Set(['loop', 'logic']);
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
        if (segments[segments.length - 1] === 'index')
            segments.pop();
        return segments.join('-').toLowerCase();
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
        await Compiler.bundleBrowserRuntimeAssets();
        // JSON manifests
        Router.reqRoutes["/routes.json"] = { GET: jsonResponse('/routes.json', ROUTES_JSON_PATH) };
        Router.reqRoutes["/shells.json"] = { GET: jsonResponse('/shells.json', SHELLS_JSON_PATH) };
        // Components must be registered before pages/layouts compile so that
        // compMapping is fully populated when import statements are generated.
        await Compiler.bundleComponents();
        await Compiler.bundlePages();
        await Promise.all([
            Compiler.bundleDependencies(),
            Compiler.bundleAssets(),
            Compiler.bundleSharedData()
        ]);
        // Write manifests after all routes are registered
        await Promise.all([
            Bun.write(Bun.file(ROUTES_JSON_PATH), JSON.stringify(Router.routeSlugs)),
            Bun.write(Bun.file(SHELLS_JSON_PATH), JSON.stringify(Compiler.layoutMapping))
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
            ? '/pages/index.js'
            : `/pages${Router.routeToFilesystemPath(pathname)}/index.js`;
    }

    /**
     * @param {string} distPath
     * @param {string} pathname
     * @returns {string}
     */
    static pageModuleFilePath(distPath, pathname) {
        return pathname === '/'
            ? path.join(distPath, 'pages', 'index.js')
            : path.join(distPath, 'pages', Router.routeToFilesystemPath(pathname).slice(1), 'index.js');
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
    /**
     * @param {CompanionScript} companion
     * @returns {string}
     */
    static companionBasePath(companion) {
        return companion.sourcePath.slice(0, -companion.provider.extension.length);
    }
    /** @param {CompanionScript} companion */
    static companionManifestPath(companion) {
        return `${Compiler.companionBasePath(companion)}.tac.json`;
    }
    /** @param {CompanionScript} companion */
    static companionWasmFallbackPath(companion) {
        return `${Compiler.companionBasePath(companion)}.wasm`;
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
        return !(/^\s*import\s+[\s\S]*?\bTac\b[\s\S]*?\bfrom\b/m.test(source)
            || /^\s*(?:const|let|var|class|function)\s+Tac\b/m.test(source));
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
    static async bundleBrowserRuntimeAssets() {
        const entrypoints = [
            `${import.meta.dir}/../runtime/spa-renderer.js`,
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
                    const mod = await import(modulePath);
                    const loaded = modules.get(id);
                    if (loaded)
                        return loaded;
                    if (typeof mod.default === 'function')
                        return mod.default;
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
        const mod = await import(pathToFileURL(filePath).href);
        if (typeof mod.default === 'function')
            return mod.default;
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
                ? layoutHTML.replace('<div id="ty-layout-slot"></div>', `<div id="ty-layout-slot">${pageHTML}</div>`)
                : pageHTML;
            const title = Compiler.escapeHTML(prerender.title || Compiler.routeTitleFallback);
            const withTitle = shellHTML.replace(/<title>.*?<\/title>/, `<title>${title}</title>`);
            const withBody = withTitle.replace('<body></body>', `<body>${bodyHTML}</body>`);
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
                    : path.join(distPath, route.slice(1), 'index.html');
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
        const data = await Compiler.extractComponents(await Bun.file(sourcePath).text(), sourcePath, 'pages', route);
        const moduleRoute = Compiler.toModuleOutputRoute(route);
        const routePath = Compiler.routePathFromPageSource(route);
        const publicPath = `/pages/${moduleRoute}`;
        await Compiler.registerModule(data, moduleRoute, 'pages', sourcePath);
        if (data.hasSlot && !Compiler.layoutMapping[routePath]?.allowSelf) {
            Compiler.layoutMapping[routePath] = {
                path: publicPath,
                allowSelf: false,
            };
        }
    }
    /** @param {string} comp */
    static async bundleComponentFile(comp) {
        comp = comp.replaceAll('\\', '/');
        const componentName = Compiler.normalizeComponentName(comp);
        const modulePath = comp.replace('.html', '.js');
        const sourcePath = `${Router.componentsPath}/${comp}`;
        if (Compiler.compMapping.has(componentName) && Compiler.compMapping.get(componentName) !== modulePath) {
            throw new Error(`Duplicate component name '${componentName}' for '${comp}' and '${Compiler.compMapping.get(componentName)}'`);
        }
        Compiler.compMapping.set(componentName, modulePath);
        const data = await Compiler.extractComponents(await Bun.file(sourcePath).text(), sourcePath, 'components', comp);
        await Compiler.registerModule(data, comp, 'components', sourcePath);
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
     * @param {string} data
     * @param {string} sourcePath
     * @param {string} dir
     * @param {string} route
     * @returns {Promise<TemplateData>}
     */
    static async extractComponents(data, sourcePath, dir, route) {
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
        let htmlContent = await rewriter.transform(new Response(data)).text();
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
            /** @param {string} name @param {string} value @param {string} hash */
            const formatAttr = (name, value, hash) => {
                if (name.startsWith('@'))
                    return `${name}="\${await ty_invokeEvent('${hash}', async ($event) => { const __event__ = $event; return ${value} })}"`;
                if (name === ':value')
                    return `value="\${ty_escapeAttr(ty_assignValue('${hash}', '${value}', ${value}))}"`;
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
                    if (!attrs.id && !resolvedComponent && tag !== 'LOOP' && tag !== 'LOGIC') {
                        attrs[':id'] = `ty_generateId('${hash}', 'id')`;
                    }
                    const attrStr = Object.entries(attrs).map(([n, v]) => formatAttr(n, v, hash)).join(' ');
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
                    if (text.text.trim() && !insideScript && !insideStyle) {
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
     * @param {TemplateData} data
     * @param {TemplateNode[]} elements
     * @returns {Promise<string>}
     */
    /** @param {TemplateData} data @param {TemplateNode[]} elements @param {string} publicPath */
    static async createJSData(data, elements, publicPath) {
        /** @type {string[]} */
        const statics = [];
        /** @type {string[]} */
        const body = [];
        for (const el of elements) {
            if (el.static)
                statics.push(el.static);
            if (el.element) {
                // Control flow and component tags are raw JS, not concatenated
                if (el.element.includes('<loop') || el.element.includes('</loop') ||
                    el.element.includes('<logic') || el.element.includes('</logic') ||
                    /<([A-Za-z0-9-]+)_\s*([\s\S]*?)\/>/.test(el.element)) {
                    body.push(el.element);
                }
                else {
                    body.push(`elements+=${el.element}`);
                }
            }
        }
        let renderSource = body.join('\n')
            .replaceAll(/`<loop :for="(.*?)">`|`<\/loop>`/g, (_, expr) => expr ? `for(${expr}) {` : '}')
            .replaceAll(/`<logic :if="(.*?)">`|`<\/logic>`/g, (_, expr) => expr ? `if(${expr}) {` : '}')
            .replaceAll(/`<logic :else-if="(.*?)">`|`<\/logic>`/g, (_, expr) => expr ? `else if(${expr}) {` : '}')
            .replaceAll(/`<logic else="">`|`<\/logic>`/g, (_, expr) => expr ? `else {` : '}');
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
                    events.push(`${key}="${value.replace(/(ty_invokeEvent\(')([^']+)(')/g, `$1${hash}$3`)}"`);
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
                if(!compRenders.has('${hash}')) {
                    render = await ${renderName}(${propsObj})
                    elements += await render(elemId, event, '${hash}')
                    compRenders.set('${hash}', render)
                } else {
                    render = compRenders.get('${hash}')
                    elements += await render(elemId, event, '${hash}')
                }
                elements += '</div>'
            `;
        });
        const rawScriptContent = await Compiler.transpileInlineScript(data.script ?? '', data.scriptLang);
        const { bindingNames: dynamicImportBindings, moduleImports: dynamicModuleImports, scriptContent, } = Compiler.liftDynamicImports(rawScriptContent);
        const moduleImports = [...dynamicModuleImports];
        const factoryBindings = [...dynamicImportBindings];
        const companionImportPath = data.companion?.importPath ?? data.companionImportPath;
        const companionLoader = companionImportPath
            ? `
const __ty_companion__ = await (async () => {
    const __ty_companion_module__ = await __ty_companion_import__();
    const __ty_Companion__ = __ty_companion_module__?.default;
    if (typeof __ty_Companion__ !== 'function') return null;
    const __ty_runtime_bindings__ = __ty_helpers__.createTacHelpers(__ty_props__);
    const __ty_instance__ = new __ty_Companion__(__ty_props__, __ty_runtime_bindings__);
    if (__ty_instance__) {
        if (__ty_instance__.__tac_wasm_ready__ && typeof __ty_instance__.__tac_wasm_ready__.then === 'function') {
            await __ty_instance__.__tac_wasm_ready__;
        }
        __ty_helpers__.bindCompanion(__ty_instance__, __ty_props__, __ty_runtime_bindings__);
    }
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
    const emit = __ty_helpers__.emit;
    const fetch = __ty_helpers__.fetch;
    const isBrowser = __ty_helpers__.isBrowser;
    const isServer = __ty_helpers__.isServer;
    const onMount = __ty_helpers__.onMount;
    const rerender = __ty_helpers__.rerender;
    const inject = __ty_helpers__.inject;
    const provide = __ty_helpers__.provide;
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

    return async function(elemId, event, compId) {
        const counters = { id: {}, ev: {}, bind: {}, persist: {} };
        const ty_componentRootId = compId
            ? (String(compId).startsWith('ty-') ? String(compId) : 'ty-' + compId + '-0')
            : null;

        __ty_helpers__.setRenderContext({ componentRootId: ty_componentRootId, elemId, event });

        const ty_generateId = (hash, source) => {
            const key = compId ? hash + '-' + compId : hash;
            const map = counters[source];

            if (key in map) {
                return 'ty-' + key + '-' + map[key]++;
            }

            map[key] = 1;
            return 'ty-' + key + '-0';
        };

        const ty_invokeEvent = async (hash, action) => {
            if (elemId === ty_generateId(hash, 'ev')) {
                if (typeof action === 'function') await action(event);
                else {
                    const toCall = (event && !action.endsWith(')')) ? action + "('" + event + "')" : action;
                    await eval(toCall);
                }
            }
            return '';
        };

        const ty_assignValue = (hash, variable) => {
            let nextValue = /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable) ? eval(variable) : '';
            if (elemId === ty_generateId(hash, 'bind') && event) {
                if (/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable)) {
                    const __val__ = event.value;
                    eval(\`\${variable} = __val__\`);
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
     * @param {{ data: TemplateData, dir: string, route: string, sourcePath: string, publicPath: string }} options
     * @returns {BunPlugin}
     */
    static createTemplatePlugin(options) {
        const { data, dir, route, sourcePath, publicPath } = options;
        const filter = Compiler.createFilePathFilter(sourcePath);
        const sourceLabel = Compiler.toSourceLabel(sourcePath);
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-template:${publicPath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    const parsed = await Compiler.parseHTML(data.html, `${dir}/${route} (${sourceLabel})`);
                    const baseCode = await Compiler.createJSData(data, parsed, publicPath);
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
        const frameworkEntryPath = `${import.meta.dir}/../runtime/tac.js`;
        const decoratorsEntryPath = `${import.meta.dir}/../runtime/decorators.js`;
        const fyloGlobalEntryPath = `${import.meta.dir}/../runtime/fylo-global.js`;
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-companion:${sourcePath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    let contents = await Bun.file(sourcePath).text();
                    const decoratorNames = Compiler.findReferencedDecorators(contents);
                    if (decoratorNames.length > 0) {
                        const importPath = Compiler.toRelativeImportPath(sourcePath, decoratorsEntryPath);
                        contents = `import { ${decoratorNames.join(', ')} } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    if (Compiler.referencesFyloGlobal(contents)) {
                        const importPath = Compiler.toRelativeImportPath(sourcePath, fyloGlobalEntryPath);
                        contents = `import { fylo } from ${JSON.stringify(importPath)};\n${contents}`;
                    }
                    if (Compiler.shouldInjectTacImport(contents)) {
                        const importPath = Compiler.toRelativeImportPath(sourcePath, frameworkEntryPath);
                        contents = `import Tac from ${JSON.stringify(importPath)};\n${contents}`;
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
     * @param {CompanionScript} companion
     * @returns {Promise<TacWasmManifest>}
     */
    static async readTacWasmManifest(companion) {
        const manifestPath = Compiler.companionManifestPath(companion);
        if (!await Bun.file(manifestPath).exists()) {
            throw new Error(`Tac Wasm companion '${Compiler.toSourceLabel(companion.sourcePath)}' requires '${Compiler.toSourceLabel(manifestPath)}'`);
        }
        /** @type {unknown} */
        const rawManifest = await Bun.file(manifestPath).json();
        if (!rawManifest || typeof rawManifest !== 'object')
            throw new Error(`Tac Wasm manifest '${Compiler.toSourceLabel(manifestPath)}' must be a JSON object`);
        const manifest = /** @type {Record<string, unknown>} */ (rawManifest);
        if (manifest.abi !== 'tac-wasm-json@1')
            throw new Error(`Tac Wasm manifest '${Compiler.toSourceLabel(manifestPath)}' must declare "abi": "tac-wasm-json@1"`);
        if (manifest.methods !== undefined && (!Array.isArray(manifest.methods) || manifest.methods.some((name) => typeof name !== 'string'))) {
            throw new Error(`Tac Wasm manifest '${Compiler.toSourceLabel(manifestPath)}' field "methods" must be an array of strings`);
        }
        if (manifest.state !== undefined && (!manifest.state || typeof manifest.state !== 'object' || Array.isArray(manifest.state))) {
            throw new Error(`Tac Wasm manifest '${Compiler.toSourceLabel(manifestPath)}' field "state" must be an object`);
        }
        return /** @type {TacWasmManifest} */ ({
            abi: 'tac-wasm-json@1',
            methods: /** @type {string[]} */ (manifest.methods ?? []),
            state: /** @type {Record<string, unknown>} */ (manifest.state ?? {}),
        });
    }
    /**
     * @param {string} command
     * @param {string} sourcePath
     * @param {string} envVar
     * @param {boolean} [includeNodeModules]
     * @returns {string | null}
     */
    static findWasmCompiler(command, sourcePath, envVar, includeNodeModules = false) {
        const override = process.env[envVar];
        if (override)
            return override;
        const executable = process.platform === 'win32' && command === 'asc' ? 'asc.cmd' : command;
        const home = process.env.HOME ?? '';
        /** @type {Record<string, string[]>} */
        const preferredCandidates = {
            asc: [path.join(home, '.bun', 'bin', executable)],
            rustc: [path.join(home, '.cargo', 'bin', executable)],
            clang: [
                '/opt/homebrew/opt/llvm/bin/clang',
                '/usr/local/opt/llvm/bin/clang',
            ],
            tinygo: [
                '/opt/homebrew/bin/tinygo',
                '/usr/local/bin/tinygo',
            ],
            wat2wasm: [
                '/opt/homebrew/bin/wat2wasm',
                '/usr/local/bin/wat2wasm',
            ],
            zig: [
                '/opt/homebrew/bin/zig',
                '/usr/local/bin/zig',
            ],
        };
        const pathCandidates = (process.env.PATH ?? '')
            .split(path.delimiter)
            .filter(Boolean)
            .map((entry) => path.join(entry, executable));
        const candidates = includeNodeModules
            ? [
                path.join(process.cwd(), 'node_modules', '.bin', executable),
                path.join(path.dirname(sourcePath), 'node_modules', '.bin', executable),
                path.join(import.meta.dir, '..', '..', 'node_modules', '.bin', executable),
                ...(preferredCandidates[command] ?? []),
                ...pathCandidates,
            ]
            : [
                ...(preferredCandidates[command] ?? []),
                ...pathCandidates,
            ];
        return candidates.find((candidate) => existsSync(candidate)) ?? null;
    }
    /** @returns {string | null} */
    static findWasmLinker() {
        return [
            '/opt/homebrew/opt/lld/bin/wasm-ld',
            '/opt/homebrew/opt/lld@21/bin/wasm-ld',
            '/usr/local/opt/lld/bin/wasm-ld',
            '/usr/local/opt/lld@21/bin/wasm-ld',
        ].find((linkerPath) => existsSync(linkerPath)) ?? null;
    }
    /**
     * @param {CompanionScript} companion
     * @param {string} languageName
     * @param {string} command
     * @param {string} envVar
     * @param {(compilerPath: string, outputPath: string) => string[]} createArgs
     * @param {string} installHint
     * @param {boolean} [includeNodeModules]
     * @returns {Promise<Uint8Array>}
     */
    static async runWasmSourceCompiler(companion, languageName, command, envVar, createArgs, installHint, includeNodeModules = false) {
        const compilerPath = Compiler.findWasmCompiler(command, companion.sourcePath, envVar, includeNodeModules);
        if (!compilerPath) {
            throw new Error(`${languageName} Tac companion '${Compiler.toSourceLabel(companion.sourcePath)}' requires '${command}' to compile to Wasm. ${installHint} You can also set ${envVar} to the compiler path.`);
        }
        const tempDir = await mkdtemp(path.join(tmpdir(), 'tachyon-tac-wasm-'));
        const outputPath = path.join(tempDir, 'companion.wasm');
        try {
            const proc = Bun.spawn(createArgs(compilerPath, outputPath), {
                cwd: process.cwd(),
                stdout: 'pipe',
                stderr: 'pipe',
            });
            const [stdout, stderr, exitCode] = await Promise.all([
                new Response(proc.stdout).text(),
                new Response(proc.stderr).text(),
                proc.exited,
            ]);
            if (exitCode !== 0) {
                const details = [stdout, stderr].filter(Boolean).join('\n').trim();
                throw new Error(`${languageName} failed to compile '${Compiler.toSourceLabel(companion.sourcePath)}'${details ? `:\n${details}` : ''}`);
            }
            return await Bun.file(outputPath).bytes();
        }
        finally {
            await rm(tempDir, { recursive: true, force: true });
        }
    }
    /**
     * @param {CompanionScript} companion
     * @returns {Promise<Uint8Array>}
     */
    static async compileWasmSourceCompanion(companion) {
        if (companion.provider.target !== 'wasm-source')
            throw new Error(`Tac companion '${Compiler.toSourceLabel(companion.sourcePath)}' is not a source-backed Wasm companion`);
        switch (companion.provider.language) {
            case 'assemblyscript':
                return Compiler.runWasmSourceCompiler(
                    companion,
                    'AssemblyScript',
                    'asc',
                    'TACHYON_WASM_ASC',
                    (compilerPath, outputPath) => [compilerPath, companion.sourcePath, '--target', 'release', '--outFile', outputPath],
                    "Install it with 'bun add -d assemblyscript'.",
                    true
                );
            case 'rust':
                return Compiler.runWasmSourceCompiler(
                    companion,
                    'Rust',
                    'rustc',
                    'TACHYON_WASM_RUSTC',
                    (compilerPath, outputPath) => [compilerPath, companion.sourcePath, '--crate-type=cdylib', '--target=wasm32-unknown-unknown', '-O', '-o', outputPath],
                    "Install Rust and add the target with 'rustup target add wasm32-unknown-unknown'."
                );
            case 'c':
                return Compiler.runWasmSourceCompiler(
                    companion,
                    'C',
                    'clang',
                    'TACHYON_WASM_CLANG',
                    (compilerPath, outputPath) => {
                        const linkerPath = Compiler.findWasmLinker();
                        return [
                            compilerPath,
                            '--target=wasm32',
                            ...(linkerPath ? [`-fuse-ld=${linkerPath}`] : []),
                            '-O3',
                            '-nostdlib',
                            '-Wl,--no-entry',
                            '-Wl,--export-memory',
                            '-Wl,--export-all',
                            '-Wl,--allow-undefined',
                            companion.sourcePath,
                            '-o',
                            outputPath,
                        ];
                    },
                    "Install LLVM/Clang with WebAssembly target support."
                );
            case 'go':
                return Compiler.runWasmSourceCompiler(
                    companion,
                    'Go',
                    'tinygo',
                    'TACHYON_WASM_TINYGO',
                    (compilerPath, outputPath) => [compilerPath, 'build', '-target', 'wasm-unknown', '-no-debug', '-opt', 'z', '-o', outputPath, companion.sourcePath],
                    "Install TinyGo; standard Go's browser Wasm target includes a Go runtime shim and is not the Tac ABI shape."
                );
            case 'zig':
                return Compiler.runWasmSourceCompiler(
                    companion,
                    'Zig',
                    'zig',
                    'TACHYON_WASM_ZIG',
                    (compilerPath, outputPath) => [compilerPath, 'build-exe', companion.sourcePath, '-target', 'wasm32-freestanding', '-fno-entry', '-rdynamic', '-O', 'ReleaseSmall', `-femit-bin=${outputPath}`],
                    'Install Zig with wasm32-freestanding target support.'
                );
            case 'wat':
                return Compiler.runWasmSourceCompiler(
                    companion,
                    'WebAssembly text',
                    'wat2wasm',
                    'TACHYON_WASM_WAT2WASM',
                    (compilerPath, outputPath) => [compilerPath, companion.sourcePath, '-o', outputPath],
                    'Install WABT to get wat2wasm.'
                );
        }
        throw new Error(`Unsupported Tac Wasm source language '${/** @type {{ language: string }} */ (companion.provider).language}' for '${Compiler.toSourceLabel(companion.sourcePath)}'`);
    }
    /**
     * @param {CompanionScript} companion
     * @returns {Promise<Uint8Array>}
     */
    static async getTacWasmBytes(companion) {
        if (companion.provider.target === 'wasm-json')
            return Bun.file(companion.sourcePath).bytes();
        if (companion.provider.target === 'wasm-source') {
            try {
                return await Compiler.compileWasmSourceCompanion(companion);
            }
            catch (error) {
                const fallbackPath = Compiler.companionWasmFallbackPath(companion);
                if (await Bun.file(fallbackPath).exists()) {
                    Compiler.compilerLogger.warn('Falling back to prebuilt Tac Wasm companion', {
                        companion: Compiler.toSourceLabel(companion.sourcePath),
                        fallback: Compiler.toSourceLabel(fallbackPath),
                        error: error instanceof Error ? error.message : String(error),
                    });
                    return Bun.file(fallbackPath).bytes();
                }
                throw error;
            }
        }
        throw new Error(`Tac companion '${Compiler.toSourceLabel(companion.sourcePath)}' is not a Wasm companion`);
    }
    /**
     * @param {string} sourcePath
     * @param {Uint8Array} wasmBytes
     * @param {TacWasmManifest} manifest
     * @returns {string}
     */
    static createTacWasmAdapterSource(sourcePath, wasmBytes, manifest) {
        const importPath = Compiler.toRelativeImportPath(sourcePath, `${import.meta.dir}/../runtime/tac.js`);
        const bytesBase64 = Buffer.from(wasmBytes).toString('base64');
        const methods = JSON.stringify(manifest.methods ?? []);
        const defaultState = JSON.stringify(manifest.state ?? {}).replace(/</g, '\\u003c');
        const sourceLabel = JSON.stringify(Compiler.toSourceLabel(sourcePath));
        return `import Tac from ${JSON.stringify(importPath)};

const __tac_wasm_source__ = ${sourceLabel};
const __tac_wasm_methods__ = ${methods};
const __tac_wasm_default_state__ = Object.freeze(${defaultState});
const __tac_wasm_bytes__ = (() => {
    const bin = atob(${JSON.stringify(bytesBase64)});
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
})();
const __tac_text_encoder__ = new TextEncoder();
const __tac_text_decoder__ = new TextDecoder();

function __tac_apply_state__(owner, message) {
    if (!message || typeof message !== 'object') return undefined;
    const state = message.state && typeof message.state === 'object' ? message.state : undefined;
    if (state) Object.assign(owner, state);
    const effects = Array.isArray(message.effects) ? message.effects : [];
    for (const effect of effects) {
        if (!effect || typeof effect !== 'object') continue;
        if (effect.type === 'emit') owner.tac.emit(String(effect.name || ''), effect.detail);
        if (effect.type === 'provide') owner.tac.provide(String(effect.key || ''), effect.value);
        if (effect.type === 'rerender') owner.tac.rerender();
    }
    return Object.prototype.hasOwnProperty.call(message, 'result') ? message.result : undefined;
}

class TacWasmJsonRuntime {
    constructor(owner) {
        this.owner = owner;
        this.exports = null;
        this.ready = this.instantiate();
    }

    async instantiate() {
        let wasmMemory;
        const memoryView = () => {
            if (!(wasmMemory instanceof WebAssembly.Memory)) {
                throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' memory is not available yet');
            }
            return new Uint8Array(wasmMemory.buffer);
        };
        const result = await WebAssembly.instantiate(__tac_wasm_bytes__, {
            env: {
                abort(messagePtr, filePtr, line, column) {
                    throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' aborted at ' + line + ':' + column);
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
                trace() {},
            },
            wasi_snapshot_preview1: {
                args_get() { return 0; },
                args_sizes_get(argcPtr, argvBufSizePtr) {
                    return 0;
                },
                clock_time_get() { return 0; },
                environ_get() { return 0; },
                environ_sizes_get() { return 0; },
                fd_close() { return 0; },
                fd_fdstat_get() { return 0; },
                fd_seek() { return 0; },
                fd_write() { return 0; },
                poll_oneoff() { return 0; },
                proc_exit(code) {
                    throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' exited with code ' + code);
                },
                random_get(ptr, len) {
                    return 0;
                },
            },
        });
        const instance = result.instance ?? result;
        this.exports = instance.exports;
        if (!(this.exports.memory instanceof WebAssembly.Memory)) {
            throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' must export memory');
        }
        wasmMemory = this.exports.memory;
        for (const name of ['alloc', 'init', 'call', 'output_ptr', 'output_len']) {
            if (typeof this.exports[name] !== 'function') {
                throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' must export ' + name + '()');
            }
        }
    }

    requireExports() {
        if (!this.exports) {
            throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' has not finished loading');
        }
        return this.exports;
    }

    writeJson(value) {
        const exports = this.requireExports();
        const bytes = __tac_text_encoder__.encode(JSON.stringify(value ?? null));
        const ptr = exports.alloc(bytes.length);
        if (!ptr) throw new Error('Tac Wasm companion ' + __tac_wasm_source__ + ' failed to allocate ' + bytes.length + ' bytes');
        new Uint8Array(exports.memory.buffer).set(bytes, ptr);
        return { ptr, len: bytes.length };
    }

    dealloc(input) {
        const exports = this.requireExports();
        if (input && typeof exports.dealloc === 'function') {
            exports.dealloc(input.ptr, input.len);
        }
    }

    readOutput() {
        const exports = this.requireExports();
        const ptr = exports.output_ptr();
        const len = exports.output_len();
        if (!len) return {};
        const memory = new Uint8Array(exports.memory.buffer);
        return JSON.parse(__tac_text_decoder__.decode(memory.subarray(ptr, ptr + len)));
    }

    init(props) {
        const exports = this.requireExports();
        const input = this.writeJson({ props });
        try {
            exports.init(input.ptr, input.len);
            return __tac_apply_state__(this.owner, this.readOutput());
        }
        finally {
            this.dealloc(input);
        }
    }

    call(method, args) {
        const exports = this.requireExports();
        const methodInput = this.writeJson(method);
        const payloadInput = this.writeJson({
            args,
            props: this.owner.props,
            state: Object.fromEntries(Object.keys(this.owner)
                .filter((key) => key !== 'props' && key !== 'tac' && !key.startsWith('__ty_'))
                .map((key) => [key, this.owner[key]])),
        });
        try {
            exports.call(methodInput.ptr, methodInput.len, payloadInput.ptr, payloadInput.len);
            return __tac_apply_state__(this.owner, this.readOutput());
        }
        finally {
            this.dealloc(methodInput);
            this.dealloc(payloadInput);
        }
    }
}

export default class extends Tac {
    constructor(props = {}, tac) {
        super(props, tac);
        Object.assign(this, structuredClone(__tac_wasm_default_state__));
        Object.defineProperty(this, '__tac_wasm_runtime__', {
            configurable: false,
            enumerable: false,
            value: new TacWasmJsonRuntime(this),
        });
        Object.defineProperty(this, '__tac_wasm_ready__', {
            configurable: false,
            enumerable: false,
            value: this.__tac_wasm_runtime__.ready.then(() => this.__tac_wasm_runtime__.init(props)),
        });
        for (const method of __tac_wasm_methods__) {
            Object.defineProperty(this, method, {
                configurable: true,
                enumerable: false,
                value: (...args) => this.__tac_wasm_runtime__.call(method, args),
            });
        }
    }
}
`;
    }
    /**
     * @param {CompanionScript} companion
     * @returns {BunPlugin}
     */
    static createWasmCompanionPlugin(companion) {
        const filter = Compiler.createFilePathFilter(companion.sourcePath);
        return /** @type {BunPlugin} */ ({
            name: `tachyon-tac-wasm-companion:${companion.sourcePath}`,
            target: /** @type {import("bun").Target} */ ('browser'),
            setup(build) {
                build.onLoad({ filter }, async () => {
                    const manifest = await Compiler.readTacWasmManifest(companion);
                    const wasmBytes = await Compiler.getTacWasmBytes(companion);
                    return {
                        contents: Compiler.createTacWasmAdapterSource(companion.sourcePath, wasmBytes, manifest),
                        loader: 'js',
                    };
                });
            }
        });
    }
    /**
     * @param {TemplateData} data
     * @param {string} sourcePath
     * @returns {BunPlugin[]}
     */
    static createCompanionPlugins(data, sourcePath) {
        if (data.companion) {
            if (data.companion.provider.target === 'ecmascript')
                return [Compiler.createCompanionScriptPlugin(data.companion.sourcePath)];
            if (data.companion.provider.target === 'wasm-json' || data.companion.provider.target === 'wasm-source')
                return [Compiler.createWasmCompanionPlugin(data.companion)];
            throw new Error(`Unsupported Tac companion target '${data.companion.provider.target}' for '${Compiler.toSourceLabel(data.companion.sourcePath)}'`);
        }
        if (data.companionImportPath) {
            const companionPath = path.resolve(path.dirname(sourcePath), data.companionImportPath);
            return [Compiler.createCompanionScriptPlugin(companionPath)];
        }
        return [];
    }
    // ── Build & register a single template module ──────────────────────────────
    /**
     * @param {TemplateData} data
     * @param {string} route
     * @param {string} dir
     * @param {string} sourcePath
     */
    static async registerModule(data, route, dir, sourcePath) {
        const publicPath = `/${dir}/${Compiler.toModuleOutputRoute(route)}`;
        if (Compiler.isGlobalOutput()) {
            const parsed = await Compiler.parseHTML(data.html, `${dir}/${route} (${Compiler.toSourceLabel(sourcePath)})`);
            const baseCode = await Compiler.createJSData(data, parsed, publicPath);
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
                ...Compiler.createCompanionPlugins(data, sourcePath),
                Compiler.createTemplatePlugin({
                    data,
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
        for (const comp of new Bun.Glob('**/*.html').scanSync({ cwd: Router.componentsPath })) {
            await Compiler.bundleComponentFile(comp);
        }
    }
    static async bundleDependencies() {
        const packageFile = Bun.file(`${process.cwd()}/package.json`);
        if (!await packageFile.exists())
            return;
        const packages = await packageFile.json();
        const modules = Object.keys(packages.dependencies ?? {});
        for (const mod of modules) {
            try {
                const result = await Bun.build({
                    // Let Bun resolve package exports/main/module fields instead
                    // of manually guessing node_modules entry files.
                    entrypoints: [mod],
                    minify: true
                });
                for (const output of result.outputs) {
                    Router.reqRoutes[`/modules/${mod}.js`] = {
                        GET: () => jsResponse(`/modules/${mod}.js`, output)
                    };
                }
            }
            catch (e) {
                Compiler.compilerLogger.warn('Failed to bundle dependency module', { module: mod, err: e });
            }
        }
    }
}
function genHash() {
    return Bun.randomUUIDv7().replace(/-/g, '').slice(-8);
}
