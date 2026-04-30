// @ts-check
/**
 * Stage 3 decorators for Tac companion scripts. Each decorator assumes the
 * decorated class extends `Tac` (or otherwise exposes a `tac` runtime-bindings
 * object on the instance), so they read and write through `this.tac.*`.
 *
 * In compiled companion scripts, these decorators are auto-imported by the
 * Tachyon compiler — user code references them as bare identifiers
 * (`@inject`, `@provide`, …). Outside the compiler (tests, library code) the
 * imports must be explicit.
 */

/**
 * @typedef {import('./tac.js').default} Tac
 */

/**
 * Field decorator. Replaces the field's initial value with `tac.inject(key, fallback)`.
 * @template T
 * @param {string} key
 * @param {T} [fallback]
 */
export function inject(key, fallback) {
    return /** @type {(value: undefined, ctx: ClassFieldDecoratorContext) => (initial: unknown) => T | undefined} */ (
        (_value, ctx) => {
            if (ctx.kind !== 'field') throw new TypeError('@inject only decorates fields');
            return /** @this {Tac} */ function () {
                return this.tac.inject(key, fallback);
            };
        }
    );
}

/**
 * Field decorator. Keeps the field's initial value and registers it via
 * `tac.provide(key, value)` after init.
 * @param {string} key
 */
export function provide(key) {
    return /** @type {(value: undefined, ctx: ClassFieldDecoratorContext) => void} */ (
        (_value, ctx) => {
            if (ctx.kind !== 'field') throw new TypeError('@provide only decorates fields');
            ctx.addInitializer(function () {
                const self = /** @type {Tac & Record<string | symbol, unknown>} */ (this);
                self.tac.provide(key, self[ctx.name]);
            });
        }
    );
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
        self.tac.onMount(() => self[ctx.name].call(self));
    });
}

/**
 * Method decorator. Triggers a re-render of the host component after the
 * decorated method settles (sync return, sync throw, promise resolve, or
 * promise reject). On the server (`!tac.isBrowser`) the underlying helper is
 * a no-op, so this is safe everywhere.
 * @param {Function} _value
 * @param {ClassMethodDecoratorContext} ctx
 */
export function render(_value, ctx) {
    if (ctx.kind !== 'method') throw new TypeError('@render only decorates methods');
    return /** @type {any} */ (/** @this {Tac} */ function (/** @type {unknown[]} */ ...args) {
        const original = /** @type {Function} */ (_value);
        const tac = this.tac;
        let result;
        try {
            result = original.apply(this, args);
        } catch (err) {
            tac.rerender();
            throw err;
        }
        if (result && typeof (/** @type {Promise<unknown>} */ (result)).then === 'function') {
            return /** @type {Promise<unknown>} */ (result).finally(() => { tac.rerender(); });
        }
        tac.rerender();
        return result;
    });
}

/**
 * Method decorator factory. Calls `tac.emit(name, returnValue)` after the
 * method runs. For async methods, emits with the resolved value; rejections
 * propagate without emitting.
 * @param {string} name
 */
export function emit(name) {
    return /** @type {<F extends Function>(value: F, ctx: ClassMethodDecoratorContext) => F} */ (
        (original, ctx) => {
            if (ctx.kind !== 'method') throw new TypeError('@emit only decorates methods');
            return /** @type {any} */ (/** @this {Tac} */ function (/** @type {unknown[]} */ ...args) {
                const result = /** @type {Function} */ (original).apply(this, args);
                const tac = this.tac;
                if (result && typeof (/** @type {Promise<unknown>} */ (result)).then === 'function') {
                    return /** @type {Promise<unknown>} */ (result).then((detail) => {
                        tac.emit(name, detail);
                        return detail;
                    });
                }
                tac.emit(name, result);
                return result;
            });
        }
    );
}
