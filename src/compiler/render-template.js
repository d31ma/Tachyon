export default async function(props) {

    // imports

    // script

    if(props) props.split(';').map(prop => eval(prop))

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

        const ty_invokeEvent = (hash, action) => {
            
            if(elemId === ty_generateId(hash, 'ev')) {
                
                if(event && !action.endsWith(')')) {
                    return action + "('" + event + "')"
                }
                return action
            }
            return "''"
        }

        const ty_assignValue = (hash, variable) => {

            if(elemId === ty_generateId(hash, 'bind') && event) {
                return variable + " = '" + event.value + "'"
            }

            return variable
        }

        let elements = '';

        let render;

        // inners

        return elements
    }
}