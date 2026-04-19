export default async function(props) {

    // imports

    let emit = () => false

    const isBrowser = typeof window !== 'undefined' && !globalThis.__ty_prerender__
    const isServer = !isBrowser

    const onMount = (fn) => {
        if (!isBrowser) return
        if (!window.__ty_onMount_queue__) window.__ty_onMount_queue__ = []
        window.__ty_onMount_queue__.push(fn)
    }

    const rerender = () => {
        if (isBrowser) window.__ty_rerender?.()
    }

    const inject = (key, fallback = undefined) => {
        if (!isBrowser) return fallback
        return window.__ty_context__?.get(key) ?? fallback
    }

    const provide = (key, value) => {
        if (isBrowser) window.__ty_context__?.set(key, value)
    }

    const persist = (key, initialValue) => {
        if (!isBrowser) return [initialValue, () => {}]
        let current = initialValue
        try {
            const stored = sessionStorage.getItem(key)
            if (stored !== null) current = JSON.parse(stored)
        } catch {}
        const save = (newValue) => {
            try { sessionStorage.setItem(key, JSON.stringify(newValue)) } catch {}
        }
        return [current, save]
    }

    // script

    if(props) {
        const __p__ = typeof props === 'string' ? JSON.parse(decodeURIComponent(props)) : props
        for(const __k__ of Object.keys(__p__)) {
            if(/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(__k__)) {
                const __v__ = __p__[__k__]
                eval(`${__k__} = __v__`)
            }
        }
    }

    const compRenders = new Map()

    return async function(elemId, event, compId) {

        const counters = { id: {}, ev: {}, bind: {} }
        const ty_componentRootId = compId
            ? (String(compId).startsWith('ty-') ? String(compId) : 'ty-' + compId + '-0')
            : null

        emit = (name, detail) => {
            const eventName = String(name || '').replace(/^@/, '')
            if(!eventName || !ty_componentRootId || typeof document === 'undefined') return false

            const target = document.getElementById(ty_componentRootId)
            if(!target || typeof CustomEvent === 'undefined') return false

            return target.dispatchEvent(new CustomEvent(eventName, {
                detail,
                bubbles: true,
                composed: true
            }))
        }

        const ty_generateId = (hash, source) => {

            const key = compId ? hash + '-' + compId : hash

            const map = counters[source]

            if(key in map) {
                return 'ty-' + key + '-' + map[key]++
            }

            map[key] = 1

            return 'ty-' + key + '-0'
        }

        const ty_invokeEvent = async (hash, action) => {
            if(elemId === ty_generateId(hash, 'ev')) {
                if(typeof action === 'function') await action(event)
                else {
                    const toCall = (event && !action.endsWith(')')) ? action + "('" + event + "')" : action
                    await eval(toCall)
                }
            }
            return ''
        }

        const ty_assignValue = (hash, variable) => {
            if(elemId === ty_generateId(hash, 'bind') && event) {
                if(/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(variable)) {
                    const __val__ = event.value
                    eval(`${variable} = __val__`)
                }
            }
            return ''
        }

        const ty_escapeHtml = (value) => {
            if(value === null || value === undefined) return ''
            return String(value)
                .replaceAll('&', '&amp;')
                .replaceAll('<', '&lt;')
                .replaceAll('>', '&gt;')
                .replaceAll('"', '&quot;')
                .replaceAll("'", '&#39;')
        }

        const ty_escapeText = ty_escapeHtml
        const ty_escapeAttr = ty_escapeHtml

        let elements = '';

        let render;

        // inners

        return elements
    }
}
