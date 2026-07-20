// @ts-check
import { describe, expect, test } from 'bun:test';
import NativeUIRuntime, { parseNativeUIFragment } from '../../src/runtime/native-ui-runtime.js';

describe('DOM-free native UI runtime', () => {
    test('parses compiler-produced HTML, event markers, keys, attributes, and inline styles', () => {
        const root = parseNativeUIFragment(`
<main style="display:flex; gap:8px">
  <button id="tc-add-0" data-tac-on-click="" data-tac-key="add">Add &amp; save</button>
  <input id="name" type="text" disabled>
</main>`);
        expect(root).toMatchObject({
            kind: 'element',
            tag: 'main',
            style: { display: 'flex', gap: '8px' },
            children: [
                {
                    tag: 'button', id: 'tc-add-0', key: 'add', events: { click: 'tc-add-0' },
                    children: [{ kind: 'text', value: 'Add & save' }],
                },
                { tag: 'input', id: 'name', attributes: { type: 'text', disabled: '' } },
            ],
        });
    });

    test('dispatches a native event through the existing Tac render closure and returns the updated snapshot', async () => {
        let count = 0;
        /** @param {string | undefined} elementId @param {{ type?: string } | undefined} event */
        const render = async (elementId, event) => {
            if (elementId === 'tc-add-0' && event?.type === 'click') count += 1;
            return `<main><p>Count: ${count}</p><button id="tc-add-0" data-tac-on-click="">Add</button></main>`;
        };
        const runtime = new NativeUIRuntime(render, { route: '/' });
        expect(JSON.stringify(await runtime.render())).toContain('Count: 0');
        const updated = await runtime.dispatch({ elementId: 'tc-add-0', type: 'click' });
        expect(JSON.stringify(updated)).toContain('Count: 1');
    });

    test('normalizes native input payloads to Tac value-event semantics', async () => {
        /** @type {unknown} */
        let received;
        const runtime = new NativeUIRuntime(async (_elementId, event) => {
            received = event;
            return '<input id="tc-name-0" data-tac-on-input="">';
        }, { route: '/' });
        await runtime.dispatch({ elementId: 'tc-name-0', type: 'input', value: 'Ada', checked: true });
        expect(received).toEqual({
            type: 'input',
            value: 'Ada',
            checked: true,
            detail: { value: 'Ada', checked: true },
            target: { value: 'Ada', checked: true },
            currentTarget: { value: 'Ada', checked: true },
        });
    });

    test('rejects malformed frames and dispatch to elements that do not advertise the event', async () => {
        expect(() => parseNativeUIFragment('<main><p>broken</main>')).toThrow(/mismatched closing tag/i);
        const runtime = new NativeUIRuntime(async () => '<main><button id="safe">Safe</button></main>', { route: '/' });
        await runtime.render();
        await expect(runtime.dispatch({ elementId: 'safe', type: 'click' })).rejects.toThrow(/does not handle 'click'/i);
    });

    test('applies the same semantic adapter mapping during live rerenders', () => {
        const root = parseNativeUIFragment(
            '<w-app-bar><span>Live</span></w-app-bar>',
            { route: '/', adapters: { 'w-app-bar': 'header' } },
        );
        expect(root).toMatchObject({
            kind: 'element',
            tag: 'header',
            adapter: 'w-app-bar',
            children: [{ kind: 'element', tag: 'span' }],
        });
    });

    test('preserves automatically inferred WebView boundaries during live rerenders', () => {
        const root = parseNativeUIFragment(
            '<main><h1>Native</h1><company-chart id="live"><canvas></canvas></company-chart></main>',
            { route: '/' },
        );
        expect(root).toMatchObject({
            kind: 'element',
            tag: 'main',
            children: [
                { kind: 'element', tag: 'h1' },
                { kind: 'webview', tag: 'company-chart', id: 'live', html: '<company-chart id="live"><canvas></canvas></company-chart>' },
            ],
        });
    });

    test('does not truncate WebView boundaries at closing-tag text inside scripts or styles', () => {
        const boundary = '<company-chart id="live"><script>const markup = "</company-chart>";</script><style>.label::after{content:"</company-chart>"}</style><canvas></canvas></company-chart>';
        const root = parseNativeUIFragment(`<main>${boundary}<p>After</p></main>`, { route: '/' });
        expect(root).toMatchObject({
            kind: 'element',
            tag: 'main',
            children: [
                { kind: 'webview', tag: 'company-chart', id: 'live', html: boundary },
                { kind: 'element', tag: 'p', children: [{ kind: 'text', value: 'After' }] },
            ],
        });
    });
});
