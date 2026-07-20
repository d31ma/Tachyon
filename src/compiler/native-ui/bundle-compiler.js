// @ts-check
import { readFile, writeFile } from 'fs/promises';
import path from 'path';
import Router from '../../server/http/route-handler.js';
import NativeUIDocumentCompiler from './document-compiler.js';
import NativeUIControllerCompiler, { NATIVE_UI_CONTROLLER_FILENAME } from './controller-compiler.js';
import { normalizeNativeUIAdapters } from './adapters.js';

export const NATIVE_UI_BUNDLE_FILENAME = 'tachyon.native-ui.json';

/** @param {unknown} value */
function resolveAdapterTags(value) {
    const adapters = normalizeNativeUIAdapters(value);
    return Array.isArray(adapters) ? adapters : Object.keys(adapters);
}

export default class NativeUIBundleCompiler {
    /**
     * @param {{ distRoot: string, routes: string[], adapters?: unknown }} options
     */
    static async compile(options) {
        const adapters = normalizeNativeUIAdapters(options.adapters);
        const routes = [];
        for (const route of [...options.routes].sort()) {
            const relativePath = route === '/'
                ? 'index.html'
                : path.join(Router.routeToFilesystemPath(route).slice(1), 'index.html');
            const html = await readFile(path.join(options.distRoot, relativePath), 'utf8');
            routes.push(await NativeUIDocumentCompiler.compile(html, { route, adapters }));
        }
        const entryRoute = routes.some((document) => document.route === '/')
            ? '/'
            : routes[0]?.route ?? '/';
        await NativeUIControllerCompiler.compile({
            routes: options.routes.map((route) => ({
                route,
                modulePath: route === '/'
                    ? path.join(options.distRoot, 'pages', 'tac.js')
                    : path.join(options.distRoot, 'pages', Router.routeToFilesystemPath(route).slice(1), 'tac.js'),
            })),
            outputFile: path.join(options.distRoot, NATIVE_UI_CONTROLLER_FILENAME),
            adapters,
        });
        const webViewFallbacks = new Set();
        /** @param {any} node */
        const visit = (node) => {
            if (!node || typeof node !== 'object') return;
            if (node.kind === 'webview' && typeof node.tag === 'string') webViewFallbacks.add(node.tag);
            for (const child of node.children ?? []) visit(child);
        };
        for (const document of routes) visit(document.root);
        const bundle = {
            schemaVersion: 1,
            renderMode: 'native',
            entryRoute,
            controller: NATIVE_UI_CONTROLLER_FILENAME,
            adapters,
            hasWebViewFallbacks: webViewFallbacks.size > 0,
            webViewFallbacks: [...webViewFallbacks].sort(),
            routes,
        };
        await writeFile(
            path.join(options.distRoot, NATIVE_UI_BUNDLE_FILENAME),
            `${JSON.stringify(bundle, null, 2)}\n`,
        );
        return bundle;
    }
}

export { resolveAdapterTags };
