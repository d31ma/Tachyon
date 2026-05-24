// @ts-check
import { Window } from 'happy-dom';
import { morphChildren, parseFragment } from '../../src/runtime/dom-helpers.js';

const STALL_THRESHOLD_MS = 10000;
const BASELINE_ROW_COUNT = 1000;
const EXPANDED_ROW_COUNT = 1500;
const TRIMMED_ROW_COUNT = 500;
const STREAM_CHUNK_COUNT = 20;
const STREAM_ROWS_PER_CHUNK = 100;

/** @typedef {{ name: string, durationMs: number, detail: Record<string, unknown> }} StressResult */

class TacDomStressSuite {
    constructor() {
        this.window = new Window({ url: 'http://localhost/' });
        this.document = this.window.document;
        this.performance = this.window.performance;
    }

    installDomGlobals() {
        Object.assign(this.window, {
            Error,
            TypeError,
            SyntaxError,
        });

        Object.assign(globalThis, {
            window: this.window,
            document: this.document,
            DOMParser: this.window.DOMParser,
            Node: this.window.Node,
            Element: this.window.Element,
            HTMLElement: this.window.HTMLElement,
            HTMLInputElement: this.window.HTMLInputElement,
            HTMLTextAreaElement: this.window.HTMLTextAreaElement,
            HTMLSelectElement: this.window.HTMLSelectElement,
            DocumentFragment: this.window.DocumentFragment,
        });
    }

    /**
     * @param {number} rowCount
     * @param {{ order?: number[], label?: string, selectedModulo?: number }} [options]
     * @returns {string}
     */
    renderRows(rowCount, options = {}) {
        const order = options.order ?? Array.from({ length: rowCount }, (_, index) => index);
        const label = options.label ?? 'initial';
        const selectedModulo = options.selectedModulo ?? 0;

        return order.map((id) => {
            const selected = selectedModulo > 0 && id % selectedModulo === 0;
            return `<article id="row-${id}" class="row${selected ? ' selected' : ''}" data-value="${id}">
            <h2>Item ${id}</h2>
            <p>${label} value ${id}</p>
            <button id="select-${id}" @click="select(${id})">Select ${id}</button>
        </article>`;
        }).join('');
    }

    /** @param {string} html */
    mountStressRoot(html) {
        this.document.body.innerHTML = `<main id="root">${html}</main>`;
    }

    /** @returns {HTMLElement} */
    getStressRoot() {
        const stressRoot = this.document.getElementById('root');
        if (!(stressRoot instanceof this.window.HTMLElement))
            throw new Error('Stress root was not mounted.');
        return /** @type {HTMLElement} */ (/** @type {unknown} */ (stressRoot));
    }

    /** @param {string} html */
    patchStressRoot(html) {
        morphChildren(this.getStressRoot(), parseFragment(html));
    }

    /**
     * @param {string} name
     * @param {() => Record<string, unknown>} fn
     * @returns {StressResult}
     */
    measure(name, fn) {
        const start = this.performance.now();
        const detail = fn();
        const durationMs = this.performance.now() - start;
        if (durationMs > STALL_THRESHOLD_MS)
            throw new Error(`${name} took ${durationMs.toFixed(1)}ms, which suggests the patcher stalled.`);
        return { name, durationMs, detail };
    }

    /** @returns {StressResult} */
    stableUpdateStress() {
        this.mountStressRoot(this.renderRows(BASELINE_ROW_COUNT));
        const sampleBefore = this.document.getElementById('row-500');

        return this.measure('stable-update-1000', () => {
            this.patchStressRoot(this.renderRows(BASELINE_ROW_COUNT, { label: 'updated', selectedModulo: 3 }));
            const sampleAfter = this.document.getElementById('row-500');
            const text = sampleAfter?.querySelector('p')?.textContent ?? '';
            if (sampleAfter !== sampleBefore)
                throw new Error('Expected stable IDs to preserve the sampled row node.');
            if (!text.includes('updated value 500'))
                throw new Error(`Expected sampled row text to update, got "${text}".`);
            return {
                rows: this.getStressRoot().childElementCount,
                preservedSample: true,
            };
        });
    }

    /** @returns {StressResult} */
    appendAndTrimStress() {
        this.mountStressRoot(this.renderRows(BASELINE_ROW_COUNT));

        return this.measure('append-trim-1000-1500-500', () => {
            this.patchStressRoot(this.renderRows(EXPANDED_ROW_COUNT, { label: 'expanded' }));
            const expanded = this.getStressRoot().childElementCount;
            this.patchStressRoot(this.renderRows(TRIMMED_ROW_COUNT, { label: 'trimmed' }));
            const trimmed = this.getStressRoot().childElementCount;
            const lastText = this.document.getElementById('row-499')?.querySelector('p')?.textContent ?? '';
            if (expanded !== EXPANDED_ROW_COUNT || trimmed !== TRIMMED_ROW_COUNT)
                throw new Error(`Expected ${EXPANDED_ROW_COUNT} then ${TRIMMED_ROW_COUNT} rows, got ${expanded} then ${trimmed}.`);
            if (!lastText.includes('trimmed value 499'))
                throw new Error(`Expected last retained row to update, got "${lastText}".`);
            return { expanded, trimmed };
        });
    }

    /** @returns {StressResult} */
    reorderStress() {
        this.mountStressRoot(this.renderRows(BASELINE_ROW_COUNT));
        const originalNodesById = new Map(Array.from(this.getStressRoot().children).map((node) => [node.id, node]));
        const reversedRowOrder = Array.from({ length: BASELINE_ROW_COUNT }, (_, index) => BASELINE_ROW_COUNT - index - 1);

        return this.measure('reverse-order-1000', () => {
            this.patchStressRoot(this.renderRows(BASELINE_ROW_COUNT, { order: reversedRowOrder, label: 'reversed' }));
            let preserved = 0;
            for (const child of Array.from(this.getStressRoot().children)) {
                if (originalNodesById.get(child.id) === child)
                    preserved += 1;
            }
            if (this.getStressRoot().firstElementChild?.id !== 'row-999')
                throw new Error('Expected first row to be row-999 after reverse patch.');
            return {
                rows: this.getStressRoot().childElementCount,
                preservedNodes: preserved,
                note: 'Tac currently reconciles lists by position, so reordered IDs are replaced rather than moved.',
            };
        });
    }

    /** @returns {StressResult} */
    streamingGrowthStress() {
        this.mountStressRoot('');

        return this.measure('stream-growth-20x100', () => {
            for (let chunk = 1; chunk <= STREAM_CHUNK_COUNT; chunk += 1) {
                this.patchStressRoot(this.renderRows(chunk * STREAM_ROWS_PER_CHUNK, { label: `chunk-${chunk}` }));
            }
            const expectedRows = STREAM_CHUNK_COUNT * STREAM_ROWS_PER_CHUNK;
            const rows = this.getStressRoot().childElementCount;
            const tail = this.document.getElementById('row-1999')?.querySelector('p')?.textContent ?? '';
            if (rows !== expectedRows)
                throw new Error(`Expected ${expectedRows} streamed rows, got ${rows}.`);
            if (!tail.includes('chunk-20 value 1999'))
                throw new Error(`Expected streamed tail text to update, got "${tail}".`);
            return { chunks: STREAM_CHUNK_COUNT, rows };
        });
    }

    /**
     * @param {StressResult[]} results
     * @returns {string}
     */
    formatResults(results) {
        const lines = ['Tac DOM stress results'];
        for (const result of results)
            lines.push(`${result.name}: ${result.durationMs.toFixed(1)}ms ${JSON.stringify(result.detail)}`);
        return `${lines.join('\n')}\n`;
    }

    run() {
        this.installDomGlobals();
        const results = [
            this.stableUpdateStress(),
            this.appendAndTrimStress(),
            this.reorderStress(),
            this.streamingGrowthStress(),
        ];
        process.stdout.write(this.formatResults(results));
    }

    close() {
        this.window.close();
    }
}

const stressSuite = new TacDomStressSuite();
try {
    stressSuite.run();
} finally {
    stressSuite.close();
}
