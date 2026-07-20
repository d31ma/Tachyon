// @ts-check

/**
 * @typedef {(props?: unknown) => Promise<TacRender> | TacRender} TacFactory
 * @typedef {(elemId?: string | null, event?: unknown, compId?: string | null) => Promise<string>} TacRender
 * @typedef {(path: string) => Promise<{ default?: TacFactory } | TacFactory>} IslandImporter
 */

/**
 * Activates independently-prerendered Tac component occurrences without
 * replacing their server-rendered DOM. The first render is deliberately
 * adopted; subsequent state changes use the renderer's component registry.
 */
export class IslandRuntime {
    /** @type {Document | Element} */
    root;
    /** @type {IslandImporter} */
    importer;
    /** @type {(callback: () => void) => unknown} */
    idle;
    /** @type {((callback: IntersectionObserverCallback) => Pick<IntersectionObserver, 'observe' | 'unobserve' | 'disconnect'>) | null} */
    observerFactory;
    /** @type {(hostId: string, render: TacRender, componentId: string) => void} */
    registerComponentRender;
    /** @type {Readonly<Record<string, string>>} */
    componentModules;
    /** @type {(error: unknown, element: HTMLElement) => void} */
    reportError;
    /** @type {(target: Element, event: Event) => void} */
    replayEvent;
    /** @type {(error: unknown, target: Element, type: string) => void} */
    onIntentError;
    /** @type {WeakMap<HTMLElement, Promise<void>>} */
    activations = new WeakMap();
    /** @type {Set<HTMLElement>} */
    scheduled = new Set();
    /** @type {Set<string>} */
    intentEventTypes = new Set();
    /** @type {WeakMap<HTMLElement, Array<{ target: Element, event: Event }>>} */
    intentQueues = new WeakMap();
    /** @type {Pick<IntersectionObserver, 'observe' | 'unobserve' | 'disconnect'> | null} */
    observer = null;
    disposed = false;
    /** @type {(event: Event) => void} */
    intentListener;

    /**
     * @param {{
     *   root?: Document | Element,
     *   importer?: IslandImporter,
     *   idle?: (callback: () => void) => unknown,
     *   observerFactory?: ((callback: IntersectionObserverCallback) => Pick<IntersectionObserver, 'observe' | 'unobserve' | 'disconnect'>) | null,
     *   componentModules?: Readonly<Record<string, string>>,
     *   registerComponentRender?: (hostId: string, render: TacRender, componentId: string) => void,
     *   reportError?: (error: unknown, element: HTMLElement) => void,
     *   replayEvent?: (target: Element, event: Event) => void,
     *   onIntentError?: (error: unknown, target: Element, type: string) => void,
     * }} [options]
     */
    constructor(options = {}) {
        this.root = options.root ?? document;
        this.importer = options.importer ?? (async (modulePath) => import(modulePath));
        this.idle = options.idle ?? ((callback) => {
            const requestIdle = /** @type {((cb: () => void) => unknown) | undefined} */ (globalThis.requestIdleCallback);
            return requestIdle ? requestIdle(callback) : setTimeout(callback, 1);
        });
        this.observerFactory = options.observerFactory ?? (
            typeof IntersectionObserver === 'function'
                ? (callback) => new IntersectionObserver(callback, { rootMargin: '100px' })
                : null
        );
        this.componentModules = options.componentModules ?? {};
        this.registerComponentRender = options.registerComponentRender ?? (() => {});
        this.reportError = options.reportError ?? ((error, element) => {
            console.error(`[tachyon] Failed to hydrate island "${element.dataset.tacScope || element.id}":`, error);
        });
        this.replayEvent = options.replayEvent ?? ((target, event) => {
            const init = {
                bubbles: event.bubbles,
                composed: event.composed,
                cancelable: event.cancelable,
                detail: /** @type {any} */ (event).detail,
                key: /** @type {any} */ (event).key,
                code: /** @type {any} */ (event).code,
                location: /** @type {any} */ (event).location,
                repeat: /** @type {any} */ (event).repeat,
                isComposing: /** @type {any} */ (event).isComposing,
                ctrlKey: /** @type {any} */ (event).ctrlKey,
                shiftKey: /** @type {any} */ (event).shiftKey,
                altKey: /** @type {any} */ (event).altKey,
                metaKey: /** @type {any} */ (event).metaKey,
                button: /** @type {any} */ (event).button,
                buttons: /** @type {any} */ (event).buttons,
                clientX: /** @type {any} */ (event).clientX,
                clientY: /** @type {any} */ (event).clientY,
                screenX: /** @type {any} */ (event).screenX,
                screenY: /** @type {any} */ (event).screenY,
                pointerId: /** @type {any} */ (event).pointerId,
                pointerType: /** @type {any} */ (event).pointerType,
                isPrimary: /** @type {any} */ (event).isPrimary,
                pressure: /** @type {any} */ (event).pressure,
                width: /** @type {any} */ (event).width,
                height: /** @type {any} */ (event).height,
                relatedTarget: /** @type {any} */ (event).relatedTarget,
                submitter: /** @type {any} */ (event).submitter,
                data: /** @type {any} */ (event).data,
                inputType: /** @type {any} */ (event).inputType,
            };
            let replay;
            try {
                replay = new /** @type {any} */ (event.constructor)(event.type, init);
            }
            catch {
                replay = new Event(event.type, init);
            }
            target.dispatchEvent(replay);
        });
        this.onIntentError = options.onIntentError ?? (() => {});
        this.intentListener = (event) => {
            const target = event.target;
            if (target && 'nodeType' in /** @type {any} */ (target))
                this.#gateIntent(/** @type {Element} */ (target), event);
        };
        for (const eventName of ['click', 'submit', 'pointerdown', 'focusin', 'keydown'])
            this.#listenForIntent(eventName);
    }

    /** @param {Document | Element} [root] */
    scan(root = this.root) {
        if (this.disposed)
            return;
        // getElementsByTagName keeps the scheduler usable in lightweight DOM
        // implementations as well as browsers; policy filtering remains exact.
        const candidates = root.getElementsByTagName('*');
        for (const candidate of candidates) {
            if (!candidate.hasAttribute('data-tac-island') || candidate.hasAttribute('data-tac-hydrated'))
                continue;
            const element = /** @type {HTMLElement} */ (candidate);
            if (this.scheduled.has(element) || this.activations.has(element))
                continue;
            this.#discoverIntentEvents(element);
            const policy = element.dataset.tacHydrate;
            if (policy === 'load') {
                void this.activate(element, 'load').catch(() => {});
            }
            else if (policy === 'idle') {
                this.scheduled.add(element);
                this.idle(() => {
                    if (this.disposed || !this.scheduled.delete(element))
                        return;
                    void this.activate(element, 'idle').catch(() => {});
                });
            }
            else if (policy === 'visible') {
                if (!this.observerFactory) {
                    void this.activate(element, 'visible-fallback').catch(() => {});
                    continue;
                }
                this.observer ??= this.observerFactory((entries) => {
                    for (const entry of entries) {
                        const observed = /** @type {HTMLElement} */ (entry.target);
                        if (!entry.isIntersecting || !observed?.dataset)
                            continue;
                        this.observer?.unobserve(observed);
                        this.scheduled.delete(observed);
                        void this.activate(observed, 'visible').catch(() => {});
                    }
                });
                this.scheduled.add(element);
                this.observer.observe(element);
            }
        }
    }

    /**
     * @param {HTMLElement} element
     * @param {string} [_reason]
     */
    activate(element, _reason = 'load') {
        const existing = this.activations.get(element);
        if (existing)
            return existing;
        const activation = this.#activate(element).catch((error) => {
            // A transient module failure must not permanently poison the
            // boundary. Coalesce the current attempt, then allow later intent
            // to retry from a clean state.
            this.activations.delete(element);
            throw error;
        });
        this.activations.set(element, activation);
        return activation;
    }

    /** @param {Element} target Activates the nearest deferred island containing an intent target. */
    async promote(target) {
        /** @type {Element | null} */
        let element = target;
        while (element && !element.hasAttribute('data-tac-island'))
            element = element.parentElement;
        if (!element || !('dataset' in element) || element.hasAttribute('data-tac-hydrated'))
            return;
        await this.activate(/** @type {HTMLElement} */ (element), 'interaction');
    }

    /** @param {string} eventName */
    #listenForIntent(eventName) {
        if (this.intentEventTypes.has(eventName) || !('addEventListener' in this.root))
            return;
        this.intentEventTypes.add(eventName);
        this.root.addEventListener(eventName, this.intentListener, true);
    }

    /** @param {HTMLElement} island */
    #discoverIntentEvents(island) {
        const nodes = [island, ...island.getElementsByTagName('*')];
        for (const node of nodes) {
            for (const attribute of node.attributes) {
                if (attribute.name.startsWith('data-tac-on-'))
                    this.#listenForIntent(attribute.name.slice(12).replaceAll('__', ':'));
            }
        }
    }

    /** @param {Element} target @param {Event} event */
    #gateIntent(target, event) {
        /** @type {Element | null} */
        let boundary = target;
        while (boundary && !boundary.hasAttribute('data-tac-island'))
            boundary = boundary.parentElement;
        if (!boundary || boundary.hasAttribute('data-tac-hydrated'))
            return;
        const island = /** @type {HTMLElement} */ (boundary);
        event.stopImmediatePropagation();
        event.stopPropagation();
        if (event.type === 'click' || event.type === 'submit')
            event.preventDefault();
        const record = { target, event };
        const queue = this.intentQueues.get(island) ?? [];
        queue.push(record);
        this.intentQueues.set(island, queue);
        if (queue.length > 1)
            return;
        void this.activate(island, 'interaction').then(() => {
            const records = this.intentQueues.get(island) ?? [];
            this.intentQueues.delete(island);
            for (const queued of records) {
                if (queued.target.isConnected)
                    this.replayEvent(queued.target, queued.event);
            }
        }).catch((error) => {
            const records = this.intentQueues.get(island) ?? [];
            this.intentQueues.delete(island);
            for (const queued of records)
                this.onIntentError(error, queued.target, queued.event.type);
        });
    }

    /** @param {HTMLElement} element */
    async #activate(element) {
        if (this.disposed)
            return;
        const modulePath = element.dataset.tacModule;
        if (!modulePath)
            throw new Error(`Island '${element.id || element.dataset.tacScope || 'unknown'}' is missing data-tac-module`);
        try {
            const scope = element.dataset.tacScope;
            const expectedModule = scope ? this.componentModules[scope] : undefined;
            if (!scope || !expectedModule || modulePath !== expectedModule) {
                throw new Error(
                    `Tac island '${element.id || scope || 'unknown'}' has module metadata that does not match the compiled component manifest`
                );
            }
            const decoded = element.dataset.tacProps
                ? JSON.parse(decodeURIComponent(element.dataset.tacProps))
                : {};
            if (!decoded || typeof decoded !== 'object' || Array.isArray(decoded))
                throw new TypeError(`Tac island '${element.id || scope}' props must decode to a JSON object`);
            for (const key of ['__proto__', 'prototype', 'constructor']) {
                if (Object.prototype.hasOwnProperty.call(decoded, key))
                    throw new TypeError(`Tac island '${element.id || scope}' props contain forbidden key '${key}'`);
            }
            const imported = await this.importer(modulePath);
            const factory = typeof imported === 'function' ? imported : imported.default;
            if (typeof factory !== 'function')
                throw new TypeError(`Tac island module '${modulePath}' does not export a component factory`);
            const render = await factory(decoded);
            if (typeof render !== 'function')
                throw new TypeError(`Tac island module '${modulePath}' did not create a render function`);
            if (this.disposed)
                return;
            const componentId = element.dataset.tacComponentId || element.id.replace(/^tc-/, '').replace(/-\d+$/, '');
            this.registerComponentRender(element.id, render, componentId);
            element.setAttribute('data-tac-hydrated', '');
            element.removeAttribute('data-tac-island-error');
            this.scheduled.delete(element);
            this.observer?.unobserve(element);
        }
        catch (error) {
            element.setAttribute('data-tac-island-error', '');
            this.reportError(error, element);
            throw error;
        }
    }

    dispose() {
        this.disposed = true;
        this.scheduled.clear();
        this.observer?.disconnect();
        this.observer = null;
        if ('removeEventListener' in this.root) {
            for (const eventName of this.intentEventTypes)
                this.root.removeEventListener(eventName, this.intentListener, true);
        }
        this.intentEventTypes.clear();
    }
}

export default IslandRuntime;
