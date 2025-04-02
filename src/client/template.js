export default async function(props) {

    // script

    if(props) props.split(';').map(prop => eval(prop))

    // imports

    return function(identifers, event, compId) {

        const elemIds = new Map()

        elemIds.set('@', new Map())
        elemIds.set('id', new Map())
        elemIds.set('bind', new Map())

        const ty_generateId = (hash, source) => {

            hash = compId ? `${hash}-${compId}` : hash

            if(elemIds.get(source).has(hash)) {

                const degree = elemIds.get(source).get(hash)

                elemIds.get(source).set(hash, degree + 1)

                return "ty-" + hash + "-" + degree
            }

            elemIds.get(source).set(hash, 1)

            return "ty-" + hash + "-0"
        }

        const ty_invokeEvent = (hash, action) => {
            
            if(identifers && identifers.elementId === ty_generateId(hash, '@')) {
                
                if(event && !action.endsWith(')')) {
                    return `${action}('${event}')`
                }
                return action
            }
            return "''"
        }

        const ty_assignValue = (hash, variable) => {

            if(identifers && identifers.elementId === ty_generateId(hash, 'bind') && event) {
                return `${variable} = '${event.value}'`
            }

            return variable
        }

        let elements = '';

        // inners

        return elements
    }
}