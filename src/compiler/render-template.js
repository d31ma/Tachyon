export default async function(props) {

    // imports

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
