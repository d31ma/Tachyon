// @ts-check
import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { IslandRuntime } from '../../src/runtime/island-runtime.js';
import { createDeferredDelegation } from '../../src/runtime/event-hydration.js';

describe('IslandRuntime', () => {
    /** @type {Window} */
    let windowInstance;
    /** @type {Array<() => void>} */
    let idleCallbacks;
    /** @type {Array<{ callback: IntersectionObserverCallback, observed: Set<Element>, disconnected: boolean }>} */
    let observers;
    /** @type {string[]} */
    let imports;
    /** @type {string[]} */
    let factoryRuns;

    beforeEach(() => {
        windowInstance = new Window({ url: 'http://localhost/' });
        idleCallbacks = [];
        observers = [];
        imports = [];
        factoryRuns = [];
    });

    afterEach(async () => windowInstance.happyDOM.close());

    function island(policy, id = `island-${policy}`) {
        const element = windowInstance.document.createElement('div');
        element.id = id;
        element.dataset.tacIsland = '';
        element.dataset.tacScope = 'counter';
        element.dataset.tacModule = '/components/counter/tac.js';
        element.dataset.tacHydrate = policy;
        element.dataset.tacProps = encodeURIComponent(JSON.stringify({ count: 2 }));
        element.innerHTML = '<button>2</button>';
        windowInstance.document.body.append(element);
        return element;
    }

    function runtime(importer = successfulImporter) {
        return new IslandRuntime({
            root: windowInstance.document,
            importer,
            componentModules: { counter: '/components/counter/tac.js' },
            reportError: () => {},
            idle: (callback) => idleCallbacks.push(callback),
            observerFactory: (callback) => {
                const record = { callback, observed: new Set(), disconnected: false };
                observers.push(record);
                return {
                    observe: (element) => record.observed.add(element),
                    unobserve: (element) => record.observed.delete(element),
                    disconnect: () => { record.disconnected = true; record.observed.clear(); },
                };
            },
        });
    }

    async function successfulImporter(path) {
        imports.push(path);
        return {
            default: async (props) => {
                expect(props).toEqual({ count: 2 });
                factoryRuns.push('counter');
                return async () => '<button>3</button>';
            },
        };
    }

    async function settle() {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
    }

    test('load islands activate immediately without replacing their SSR root', async () => {
        const element = island('load');
        const originalButton = element.firstElementChild;
        const subject = runtime();

        subject.scan();
        await settle();

        expect(imports).toEqual(['/components/counter/tac.js']);
        expect(factoryRuns).toEqual(['counter']);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
        expect(element.firstElementChild).toBe(originalButton);
    });

    test('idle islands wait for the injected idle scheduler', async () => {
        const element = island('idle');
        const subject = runtime();
        subject.scan();

        expect(imports).toEqual([]);
        expect(idleCallbacks).toHaveLength(1);
        idleCallbacks[0]();
        await settle();
        expect(imports).toHaveLength(1);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
    });

    test('visible islands activate only after intersecting', async () => {
        const element = island('visible');
        const subject = runtime();
        subject.scan();

        expect(imports).toEqual([]);
        expect(observers[0].observed.has(element)).toBe(true);
        observers[0].callback(
            /** @type {any} */ ([{ target: element, isIntersecting: true }]),
            /** @type {any} */ (null),
        );
        await settle();
        expect(imports).toHaveLength(1);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
    });

    test('interaction islands remain dormant until explicitly activated', async () => {
        const element = island('interaction');
        const subject = runtime();
        subject.scan();
        await settle();
        expect(imports).toEqual([]);

        await subject.activate(element, 'interaction');
        expect(imports).toHaveLength(1);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
    });

    test('a captured direct click bootstraps activation and replays without prior pointer or focus intent', async () => {
        const element = island('interaction');
        const button = /** @type {Element} */ (element.firstElementChild);
        const subject = runtime();
        const capture = {
            queue: [{ type: 'click', target: button }],
            onIntent: null,
            stopped: false,
            stop() { this.stopped = true; },
        };
        /** @type {string[]} */
        const replayed = [];
        const delegation = createDeferredDelegation({
            ensure: () => {},
            requestIdle: () => {},
            getCapture: () => capture,
            beforeReplay: (target) => subject.promote(target),
            dispatch: (_target, type) => replayed.push(type),
        });

        // SPA startup always schedules click for navigation, even before an
        // interaction-island module has registered its own event manifest.
        delegation.schedule(['click']);
        await delegation.flush();

        expect(imports).toHaveLength(1);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
        expect(replayed).toEqual(['click']);
        expect(capture.queue).toEqual([]);
        expect(capture.stopped).toBe(true);
    });

    test('post-startup intent waits for a cold module and replays exactly once after activation', async () => {
        const element = island('interaction');
        const button = /** @type {Element} */ (element.firstElementChild);
        button.setAttribute('data-tac-on-click', '');
        /** @type {((value: any) => void) | null} */
        let resolveImport = null;
        const importer = (path) => {
            imports.push(path);
            return new Promise((resolve) => { resolveImport = resolve; });
        };
        const subject = runtime(importer);
        subject.scan();
        let handled = 0;
        button.addEventListener('click', () => { handled += 1; });

        const original = new windowInstance.Event('click', { bubbles: true, cancelable: true });
        button.dispatchEvent(original);
        expect(original.defaultPrevented).toBe(true);
        expect(handled).toBe(0);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(false);

        /** @type {any} */ (resolveImport)({
            default: async () => async () => '<button>ready</button>',
        });
        await settle();

        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
        expect(handled).toBe(1);
        expect(imports).toHaveLength(1);
    });

    test('cold-import replay preserves modified non-primary click navigation semantics', async () => {
        const element = island('interaction');
        element.innerHTML = '<a href="/docs" data-tac-on-click="">Docs</a>';
        const anchor = /** @type {Element} */ (element.firstElementChild);
        /** @type {((value: any) => void) | null} */
        let resolveImport = null;
        const subject = runtime((path) => {
            imports.push(path);
            return new Promise((resolve) => { resolveImport = resolve; });
        });
        subject.scan();
        /** @type {MouseEvent[]} */
        const handled = [];
        anchor.addEventListener('click', (event) => handled.push(/** @type {MouseEvent} */ (event)));

        anchor.dispatchEvent(new windowInstance.MouseEvent('click', {
            bubbles: true,
            cancelable: true,
            ctrlKey: true,
            shiftKey: true,
            button: 1,
            buttons: 4,
            clientX: 42,
            clientY: 24,
        }));
        expect(handled).toHaveLength(0);

        /** @type {any} */ (resolveImport)({
            default: async () => async () => '<a href="/docs">Docs</a>',
        });
        await settle();

        expect(handled).toHaveLength(1);
        expect(handled[0].ctrlKey).toBe(true);
        expect(handled[0].shiftKey).toBe(true);
        expect(handled[0].button).toBe(1);
        expect(handled[0].buttons).toBe(4);
        expect(handled[0].clientX).toBe(42);
        expect(handled[0].clientY).toBe(24);
        const spaEligible = !handled[0].ctrlKey && !handled[0].metaKey
            && !handled[0].shiftKey && !handled[0].altKey && handled[0].button === 0;
        expect(spaEligible).toBe(false);
    });

    test('post-startup intent failure preserves SSR and invokes native recovery once', async () => {
        const element = island('interaction');
        element.innerHTML = '<a href="/fallback">Continue</a>';
        const anchor = /** @type {Element} */ (element.firstElementChild);
        const failures = [];
        const subject = new IslandRuntime({
            root: windowInstance.document,
            importer: async () => { throw new Error('offline'); },
            componentModules: { counter: '/components/counter/tac.js' },
            reportError: () => {},
            onIntentError: (error, target, type) => failures.push({ error, target, type }),
        });
        const originalAnchor = element.firstElementChild;

        anchor.dispatchEvent(new windowInstance.Event('click', { bubbles: true, cancelable: true }));
        await settle();

        expect(failures).toHaveLength(1);
        expect(failures[0].target).toBe(anchor);
        expect(failures[0].type).toBe('click');
        expect(element.firstElementChild).toBe(originalAnchor);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(false);
        expect(element.hasAttribute('data-tac-island-error')).toBe(true);
        subject.dispose();
    });

    test('static islands are ignored', async () => {
        const element = island('never');
        element.removeAttribute('data-tac-island');
        element.dataset.tacIslandStatic = '';
        element.removeAttribute('data-tac-module');
        element.removeAttribute('data-tac-props');
        const subject = runtime();

        subject.scan();
        await settle();
        expect(imports).toEqual([]);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(false);
    });

    test('concurrent activation imports and mounts an island exactly once', async () => {
        const element = island('interaction');
        const subject = runtime();

        await Promise.all([
            subject.activate(element, 'interaction'),
            subject.activate(element, 'interaction'),
            subject.activate(element, 'interaction'),
        ]);
        expect(imports).toHaveLength(1);
        expect(factoryRuns).toHaveLength(1);
    });

    test('failed activation is observable and does not mark the island hydrated', async () => {
        const element = island('interaction');
        let attempts = 0;
        const subject = runtime(async (path) => {
            attempts += 1;
            if (attempts === 1)
                throw new Error('network unavailable');
            return successfulImporter(path);
        });

        await expect(subject.activate(element, 'interaction')).rejects.toThrow(/network unavailable/);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(false);
        expect(element.hasAttribute('data-tac-island-error')).toBe(true);
        await subject.activate(element, 'interaction');
        expect(attempts).toBe(2);
        expect(element.hasAttribute('data-tac-hydrated')).toBe(true);
        expect(element.hasAttribute('data-tac-island-error')).toBe(false);
    });

    test.each([
        ['https://evil.example/island.js', 'counter'],
        ['//evil.example/island.js', 'counter'],
        ['/components/../pages/tac.js', 'counter'],
        ['/pages/counter/tac.js', 'counter'],
        ['/components/counter/tac.js', 'other'],
    ])('rejects untrusted module metadata %s before import', async (modulePath, scope) => {
        const element = island('interaction');
        element.dataset.tacModule = modulePath;
        element.dataset.tacScope = scope;
        let importerCalled = false;
        const subject = runtime(async () => {
            importerCalled = true;
            return successfulImporter(modulePath);
        });

        await expect(subject.activate(element)).rejects.toThrow(/compiled component manifest/);
        expect(importerCalled).toBe(false);
    });

    test.each([
        ['[]', /JSON object/],
        ['"string"', /JSON object/],
        ['{"constructor":{}}', /forbidden key 'constructor'/],
        ['{"__proto__":{}}', /forbidden key '__proto__'/],
    ])('rejects unsafe props %s before import', async (props, expected) => {
        const element = island('interaction');
        element.dataset.tacProps = encodeURIComponent(props);
        let importerCalled = false;
        const subject = runtime(async () => {
            importerCalled = true;
            return successfulImporter('/components/counter/tac.js');
        });

        await expect(subject.activate(element)).rejects.toThrow(expected);
        expect(importerCalled).toBe(false);
    });

    test('dispose disconnects observers and prevents scheduled activation', async () => {
        island('visible');
        island('idle', 'idle-two');
        const subject = runtime();
        subject.scan();

        subject.dispose();
        expect(observers.every((observer) => observer.disconnected)).toBe(true);
        idleCallbacks[0]();
        await settle();
        expect(imports).toEqual([]);
    });
});
