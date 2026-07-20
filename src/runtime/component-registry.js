// @ts-check
/**
 * Component-scoped re-render registry.
 *
 * Every rendered component registers its host element id → its render closure,
 * so an interaction or state change inside a component can re-render just that
 * subtree instead of the whole page (generalizing the `lazy` boundary to all
 * components). The render/morph itself stays in the renderer; this module owns
 * only the lookup + the safety decision.
 *
 * Safety: a component is re-rendered in isolation only when its host id is its
 * canonical render root (`tc-<compId>-0`). The compiler gives each repeated
 * occurrence a distinct compId. The duplicate-compId guard remains defensive
 * for manually registered or legacy renderers and falls back to a parent/page
 * render when isolation cannot be proven safe.
 *
 * @typedef {(elemId?: string | null, event?: unknown, compId?: string | null) => Promise<string>} TacRender
 * @typedef {{ render: TacRender, compId: string }} ComponentEntry
 */

export function createComponentRegistry() {
    /** @type {Map<string, ComponentEntry>} */
    const renders = new Map();
    /** Duplicate compIds seen at more than one host — not safely scopable. */
    const repeatedCompIds = new Set();
    /** @type {Map<string, string>} compId → first host id seen. */
    const firstHostForCompId = new Map();

    /**
     * @param {string} hostId
     * @param {TacRender} render
     * @param {string} compId
     */
    function register(hostId, render, compId) {
        if (!hostId || typeof render !== 'function' || typeof compId !== 'string')
            return;
        renders.set(hostId, { render, compId });
        const seen = firstHostForCompId.get(compId);
        if (seen === undefined)
            firstHostForCompId.set(compId, hostId);
        else if (seen !== hostId)
            repeatedCompIds.add(compId); // Legacy/manual duplicate scope.
    }

    /**
     * The entry for a host id, only when it is safe to re-render in isolation.
     * @param {string} hostId
     * @returns {ComponentEntry | null}
     */
    function scopable(hostId) {
        const entry = renders.get(hostId);
        if (!entry || repeatedCompIds.has(entry.compId) || hostId !== `tc-${entry.compId}-0`)
            return null;
        return entry;
    }

    /**
     * The component that *owns the handler* on the trigger element — i.e. the
     * nearest scopable component host **strictly above** the trigger. The walk
     * starts at the trigger's parent on purpose: a handler placed on a child
     * component's host tag (`<child on:done="parentMethod()">`) belongs to the
     * parent's render, not the child, so it must scope to the parent.
     * @param {string | null | undefined} elementId
     * @returns {{ host: HTMLElement, entry: ComponentEntry } | null}
     */
    function findAncestor(elementId) {
        const trigger = elementId ? document.getElementById(elementId) : null;
        /** @type {HTMLElement | null} */
        let element = trigger ? trigger.parentElement : null;
        while (element) {
            const entry = scopable(element.id);
            if (entry)
                return { host: element, entry };
            element = element.parentElement;
        }
        return null;
    }

    return { register, scopable, findAncestor };
}
