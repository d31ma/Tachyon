// @ts-check
import { describe, expect, test } from 'bun:test';
import NativeUIDocumentCompiler from '../../src/compiler/native-ui/document-compiler.js';

describe('NativeUIDocumentCompiler', () => {
    test('lowers strict HTML into a versioned native UI document without inventing native authoring tags', async () => {
        const document = await NativeUIDocumentCompiler.compile(`<!doctype html>
<html><head><title>Products</title></head><body>
  <main id="products" class="stack" style="display:flex; flex-direction:column; gap:12px">
    <h1>Products</h1>
    <button id="tc-buy-0" data-tac-on-click="">Buy</button>
  </main>
</body></html>`, { route: '/' });

        expect(document).toEqual({
            schemaVersion: 1,
            route: '/',
            title: 'Products',
            root: {
                kind: 'element',
                tag: 'main',
                id: 'products',
                key: null,
                adapter: null,
                attributes: { class: 'stack' },
                style: { display: 'flex', 'flex-direction': 'column', gap: '12px' },
                events: {},
                children: [
                    {
                        kind: 'element', tag: 'h1', id: null, key: null, adapter: null,
                        attributes: {}, style: {}, events: {},
                        children: [{ kind: 'text', value: 'Products' }],
                    },
                    {
                        kind: 'element', tag: 'button', id: 'tc-buy-0', key: null, adapter: null,
                        attributes: {}, style: {}, events: { click: 'tc-buy-0' },
                        children: [{ kind: 'text', value: 'Buy' }],
                    },
                ],
            },
        });
    });

    test('preserves multiple body roots through an internal fragment rather than requiring authored wrapper tags', async () => {
        const document = await NativeUIDocumentCompiler.compile('<body><h1>One</h1><p>Two</p></body>', { route: '/two' });
        expect(document.root).toMatchObject({ kind: 'fragment' });
        expect(document.root.children.map((child) => child.tag)).toEqual(['h1', 'p']);
    });

    test('fails when a compiler control tag survives lowering', async () => {
        await expect(NativeUIDocumentCompiler.compile(
            '<body><loop :for="item of items"><p>{item}</p></loop></body>',
            { route: '/' },
        )).rejects.toThrow(/unresolved Tac <loop>/i);
    });

    test('fails closed for genuinely unknown non-HTML tags', async () => {
        await expect(NativeUIDocumentCompiler.compile('<body><mystery>Unknown</mystery></body>', { route: '/' }))
            .rejects.toThrow(/unknown HTML element <mystery>/i);
    });

    test('retains custom-element HTML when a native adapter is registered', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><duvay-chart id="sales" title="Sales"></duvay-chart></body>',
            { route: '/', adapters: ['duvay-chart'] },
        );
        expect(document.root).toMatchObject({
            kind: 'element',
            tag: 'duvay-chart',
            id: 'sales',
            adapter: 'duvay-chart',
            attributes: { title: 'Sales' },
        });
    });

    test('lowers a custom element through an explicit semantic native adapter', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><w-app-bar id="primary"><span>Tachyon</span></w-app-bar></body>',
            { route: '/', adapters: { 'w-app-bar': 'header' } },
        );
        expect(document.root).toMatchObject({
            kind: 'element',
            tag: 'header',
            id: 'primary',
            adapter: 'w-app-bar',
            children: [{ kind: 'element', tag: 'span' }],
        });
    });

    test('rejects adapter mappings to elements without a native schema mapping', async () => {
        await expect(NativeUIDocumentCompiler.compile(
            '<body><w-canvas></w-canvas></body>',
            { route: '/', adapters: { 'w-canvas': 'canvas' } },
        )).rejects.toThrow(/adapter.*w-canvas.*canvas.*schema-v1/i);
    });

    test('automatically captures the nearest unmapped subtree as WebView HTML', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><main><h1>Sales</h1><company-chart id="sales"><canvas aria-label="Chart"></canvas></company-chart><p>Native footer</p></main></body>',
            { route: '/' },
        );
        expect(document.root).toMatchObject({
            kind: 'element',
            tag: 'main',
            children: [
                { kind: 'element', tag: 'h1' },
                {
                    kind: 'webview',
                    tag: 'company-chart',
                    id: 'sales',
                    attributes: {},
                    html: '<company-chart id="sales"><canvas aria-label="Chart"></canvas></company-chart>',
                },
                { kind: 'element', tag: 'p' },
            ],
        });
    });

    test('automatically creates a boundary for known standard HTML without a native mapping', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><main><canvas aria-label="Chart"></canvas><p>Native</p></main></body>',
            { route: '/' },
        );
        expect(document.root).toMatchObject({
            kind: 'element',
            tag: 'main',
            children: [
                { kind: 'webview', tag: 'canvas', html: '<canvas aria-label="Chart"></canvas>' },
                { kind: 'element', tag: 'p' },
            ],
        });
    });

    test('keeps root-relative boundary assets inside the packaged app resource root', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><company-logo><img src="/shared/logo.svg"><a href="/docs">Docs</a></company-logo></body>',
            { route: '/' },
        );
        expect(document.root).toMatchObject({
            kind: 'webview',
            html: '<company-logo><img src="shared/logo.svg"><a href="docs">Docs</a></company-logo>',
        });
    });

    test('falls back a native tag whose browser behavior has no native adapter', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><main><h1>Native</h1><nav w-dropdown><a href="/docs">Docs</a></nav></main></body>',
            { route: '/' },
        );
        expect(document.root).toMatchObject({
            kind: 'element',
            tag: 'main',
            children: [
                { kind: 'element', tag: 'h1' },
                { kind: 'webview', tag: 'nav', html: '<nav w-dropdown=""><a href="docs">Docs</a></nav>' },
            ],
        });
    });

    test('preserves ancestor component scopes inside an isolated boundary', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><div data-tac-scope="menu"><nav class="drawer" w-dropdown>Menu</nav></div></body>',
            { route: '/' },
        );
        expect(document.root.children[0]).toMatchObject({
            kind: 'webview',
            html: '<div data-tac-scope="menu"><nav class="drawer" w-dropdown="">Menu</nav></div>',
        });
    });

    test('carries component-scoped CSS into the isolated boundary document', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><div data-tac-scope="menu"><style>@scope ([data-tac-scope="menu"]) {.drawer{display:none}}</style><nav class="drawer" w-dropdown>Menu</nav></div></body>',
            { route: '/' },
        );
        expect(document.root.children[0].html).toBe(
            '<style>@scope ([data-tac-scope="menu"]) {.drawer{display:none}}</style><div data-tac-scope="menu"><nav class="drawer" w-dropdown="">Menu</nav></div>',
        );
    });

    test('does not double-encode text entities inside a WebView boundary', async () => {
        const document = await NativeUIDocumentCompiler.compile(
            '<body><code-panel>one &amp;&amp; two &quot;quoted&quot;</code-panel></body>',
            { route: '/' },
        );
        expect(document.root.html).toBe('<code-panel>one &amp;&amp; two &quot;quoted&quot;</code-panel>');
    });
});
