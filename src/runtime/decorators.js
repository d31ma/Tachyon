// @ts-check
/**
 * Stage 3 decorators for Tac companion scripts. Each decorator assumes the
 * decorated class extends `Tac` (or otherwise exposes a `tac` runtime-bindings
 * object on the instance), so they read and write through `this.tac.*`.
 *
 * In compiled companion scripts, these decorators are auto-imported by the
 * Tachyon compiler — user code references them as bare identifiers
 * (`@subscribe`, `@publish`, …). Outside the compiler (tests, library code) the
 * imports must be explicit.
 */

/**
 * @typedef {import('./tac.js').default} Tac
 */

/**
 * @typedef {{ name: string, field: string | symbol, options: { retain: true } & Record<string, unknown> }} TacPublishedField
 */

/**
 * @param {Tac & { __tc_signal_publish_fields__?: TacPublishedField[] }} self
 * @param {string} name
 * @param {string | symbol} field
 * @param {Record<string, unknown>} options
 */
function registerPublishedField(self, name, field, options) {
    if (!Object.prototype.hasOwnProperty.call(self, '__tc_signal_publish_fields__')) {
        Object.defineProperty(self, '__tc_signal_publish_fields__', {
            configurable: true,
            enumerable: false,
            value: [],
            writable: true,
        });
    }
    const fields = /** @type {TacPublishedField[]} */ (self.__tc_signal_publish_fields__);
    fields.push({
        name,
        field,
        options: { ...options, retain: true },
    });
}

/**
 * @typedef {{ onMount?: boolean }} TacSubscribeMethodOptions
 */

/**
 * @param {unknown} value
 * @returns {value is ClassFieldDecoratorContext | ClassMethodDecoratorContext}
 */
function isDecoratorContext(value) {
    return Boolean(value && typeof value === 'object' && 'kind' in value && 'name' in value);
}

/**
 * @param {string | symbol} name
 */
function memberSignalName(name) {
    return String(name);
}

/**
 * @param {unknown} name
 * @param {ClassFieldDecoratorContext | ClassMethodDecoratorContext} ctx
 */
function resolveSignalName(name, ctx) {
    return typeof name === 'string' && name.length > 0
        ? name
        : memberSignalName(ctx.name);
}

/**
 * @param {unknown} value
 * @returns {value is Record<string, unknown>}
 */
function isPlainOptions(value) {
    return Boolean(value && typeof value === 'object' && !Array.isArray(value));
}

/**
 * @param {unknown} nameOrOptions
 * @param {unknown} fallbackOrOptions
 * @returns {TacSubscribeMethodOptions}
 */
function resolveSubscribeMethodOptions(nameOrOptions, fallbackOrOptions) {
    if (typeof nameOrOptions === 'string') {
        return isPlainOptions(fallbackOrOptions)
            ? /** @type {TacSubscribeMethodOptions} */ (fallbackOrOptions)
            : {};
    }
    return isPlainOptions(nameOrOptions)
        ? /** @type {TacSubscribeMethodOptions} */ (nameOrOptions)
        : {};
}

/**
 * @param {unknown} nameOrOptions
 * @param {unknown} options
 * @returns {{ retain?: boolean }}
 */
function resolvePublishOptions(nameOrOptions, options) {
    if (typeof nameOrOptions === 'string') {
        return isPlainOptions(options) ? /** @type {{ retain?: boolean }} */ (options) : {};
    }
    return isPlainOptions(nameOrOptions) ? /** @type {{ retain?: boolean }} */ (nameOrOptions) : {};
}

/**
 * @template T
 * @param {unknown} nameOrOptions
 * @param {T | TacSubscribeMethodOptions | undefined} fallbackOrOptions
 * @param {undefined | Function} _value
 * @param {ClassFieldDecoratorContext | ClassMethodDecoratorContext} ctx
 */
function applySubscribe(nameOrOptions, fallbackOrOptions, _value, ctx) {
    const name = resolveSignalName(nameOrOptions, ctx);
    if (ctx.kind === 'method') {
        const options = resolveSubscribeMethodOptions(nameOrOptions, fallbackOrOptions);
        ctx.addInitializer(/** @this {Tac & Record<string | symbol, unknown>} */ function () {
            const self = /** @type {Tac & Record<string | symbol, unknown>} */ (this);
            queueMicrotask(() => {
                self.tac.subscribe(name, (/** @type {unknown} */ value) => {
                    const handler = /** @type {Function} */ (self[ctx.name]);
                    handler.call(self, value);
                }, { immediate: false });
                if (options.onMount) {
                    self.tac.onMount(() => {
                        const handler = /** @type {Function} */ (self[ctx.name]);
                        return handler.call(self);
                    });
                }
            });
        });
        return;
    }
    if (ctx.kind !== 'field') throw new TypeError('@subscribe only decorates fields or methods');
    ctx.addInitializer(/** @this {Tac & Record<string | symbol, unknown>} */ function () {
        const self = /** @type {Tac & Record<string | symbol, unknown>} */ (this);
        queueMicrotask(() => {
            self.tac.subscribe(name, (/** @type {unknown} */ value) => {
                self[ctx.name] = value;
            }, { immediate: true });
        });
    });
    return /** @this {Tac} */ function () {
        return /** @type {T | undefined} */ (this.tac.subscribe(name, fallbackOrOptions));
    };
}

/**
 * Field decorator. Replaces the field's initial value with the retained signal
 * value, falls back when the signal has not been published, and subscribes the
 * field to future publications. Method decorator. Subscribes the method to
 * future publications and can optionally run it once on mount.
 * @template T
 * @param {string | T | TacSubscribeMethodOptions | undefined} [name]
 * @param {T | TacSubscribeMethodOptions} [fallbackOrOptions]
 */
export function subscribe(name, fallbackOrOptions) {
    if (isDecoratorContext(fallbackOrOptions)) {
        return applySubscribe(undefined, undefined, /** @type {undefined | Function} */ (name), fallbackOrOptions);
    }
    return /** @type {any} */ (
        (/** @type {undefined | Function} */ _value, /** @type {ClassFieldDecoratorContext | ClassMethodDecoratorContext} */ ctx) => applySubscribe(name, fallbackOrOptions, _value, ctx)
    );
}

/**
 * @param {unknown} nameOrOptions
 * @param {{ retain?: boolean } | undefined} options
 * @param {undefined | Function} value
 * @param {ClassFieldDecoratorContext | ClassMethodDecoratorContext} ctx
 */
function applyPublish(nameOrOptions, options, value, ctx) {
    const name = resolveSignalName(nameOrOptions, ctx);
    const publishOptions = resolvePublishOptions(nameOrOptions, options);
    if (ctx.kind === 'field') {
        ctx.addInitializer(/** @this {Tac & { __tc_signal_publish_fields__?: TacPublishedField[] }} */ function () {
            const self = /** @type {Tac & { __tc_signal_publish_fields__?: TacPublishedField[] }} */ (this);
            registerPublishedField(self, name, ctx.name, publishOptions);
        });
        return;
    }
    if (ctx.kind !== 'method') throw new TypeError('@publish only decorates fields or methods');
    return /** @type {any} */ (/** @this {Tac} */ function (/** @type {unknown[]} */ ...args) {
        const result = /** @type {Function} */ (value).apply(this, args);
        const tac = this.tac;
        if (result && typeof (/** @type {Promise<unknown>} */ (result)).then === 'function') {
            return /** @type {Promise<unknown>} */ (result).then((detail) => {
                tac.publish(name, detail, publishOptions);
                return detail;
            });
        }
        tac.publish(name, result, publishOptions);
        return result;
    });
}

/**
 * Field or method decorator factory. On fields, publishes the retained value
 * after construction and after future assignments. On methods, publishes the
 * return value after the method runs; async rejections propagate without
 * publishing.
 * @param {string | { retain?: boolean } | undefined | Function} [name]
 * @param {{ retain?: boolean } | ClassFieldDecoratorContext | ClassMethodDecoratorContext} [options]
 */
export function publish(name, options = {}) {
    if (isDecoratorContext(options)) {
        return applyPublish(undefined, undefined, /** @type {undefined | Function} */ (name), options);
    }
    return /** @type {any} */ ((/** @type {undefined | Function} */ value, /** @type {ClassFieldDecoratorContext | ClassMethodDecoratorContext} */ ctx) => applyPublish(name, /** @type {{ retain?: boolean } | undefined} */ (options), value, ctx));
}

/**
 * Field decorator. Replaces the field's initial value with `tac.env(key, fallback)`.
 * @template T
 * @param {string} key
 * @param {T} [fallback]
 */
export function env(key, fallback) {
    return /** @type {(value: undefined, ctx: ClassFieldDecoratorContext) => (initial: unknown) => T | undefined} */ (
        (_value, ctx) => {
            if (ctx.kind !== 'field') throw new TypeError('@env only decorates fields');
            return /** @this {Tac} */ function () {
                return this.tac.env(key, fallback);
            };
        }
    );
}

/**
 * Method decorator. Registers the decorated method as an `onMount` handler
 * bound to the instance.
 * @param {Function} _value
 * @param {ClassMethodDecoratorContext} ctx
 */
export function onMount(_value, ctx) {
    if (ctx.kind !== 'method') throw new TypeError('@onMount only decorates methods');
    ctx.addInitializer(function () {
        const self = /** @type {Tac & Record<string | symbol, Function>} */ (this);
        // Renderer binding occurs after class initializers, including when an
        // app constructor omits the injected Tac helper argument.
        queueMicrotask(() => self.tac.onMount(() => self[ctx.name].call(self)));
    });
}
