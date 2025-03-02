/** @type {Function} */
let render
/** @type {Timer} */ 
let interval
const elementsVar = 'elements';
/** @type {Map<string, Record<string, number>>} */ 
const routes = new Map()
let params = []
/** @type {Record<string, any>} */ 
const slugs = {}
let currentTemplate = ''
let previousRender = ''
let currentScript = ''

let counter = 0
let elementId;
let selectionStart;

if(routes.size === 0) {

    fetch('/routes.json')
        .then(res => res.json())
        .then(data => {
            for (const [path, slugs] of Object.entries(data)) {
                routes.set(path, slugs)
            }
            setPageTemplate(window.location.pathname)
        })
}

function loopRender() {

    const loop = () => {
        if(render) mergeBodyHTML(render())
    }

    return setInterval(() => {
        queueMicrotask(loop)
    }, 200)
}

/**
 * @param {string} html
 * @param {HTMLScriptElement} script
 * @param {HTMLStyleElement | undefined} style
 */
function mergeBodyHTML(html, style) {

    if(html === previousRender) return

    const styles = document.head.getElementsByTagName('style')
    if(styles.length > 0) styles[0].remove()
    if(style) document.head.appendChild(style)

    removeEvents()

    previousRender = html
    document.body.innerHTML = html

    counter = 0

    addEvents()

    if(elementId) {

        const element = document.getElementById(elementId)

        if(element) {

            element.focus()

            if(selectionStart && 'setSelectionRange' in element) {
                element.setSelectionRange(selectionStart, selectionStart)
            }
        }

        elementId = null;
        selectionStart = null
    }
}

/**
 * @param {HTMLCollection | undefined} elements
 */
function removeEvents(elements) {

    if(!elements) elements = document.body.children

    for (const element of elements) {

        for (const attribute of element.attributes) {

            if (attribute.name.startsWith('@')) {

                const event = attribute.name.substring(1)

                element.removeEventListener(event, (e) => {})
            }
        }

        removeEvents(element.children)
    }
}

/**
 * @param {HTMLCollection | undefined} elements
 */
function addEvents(elements) {

    if(!elements) elements = document.body.children

    for (const element of elements) {

        for (const attribute of element.attributes) {

            if (attribute.name.startsWith('@')) {

                element.id ||= ++counter

                const event = attribute.name.substring(1)

                element.addEventListener(event, (e) => {

                    if(interval) clearInterval(interval)

                    const [ compId, classId ] = element.classList.values().filter(val => val.startsWith('ty-')).toArray()

                    elementId = element.id
                    selectionStart = e.target.selectionStart

                    const func = attribute.value.includes('=>') ? `(${attribute.value})("${e.target.value}", "${e.target.defaultValue}")` : attribute.value

                    const execute = { compId, classId, func }

                    mergeBodyHTML(render(execute))

                    interval = loopRender()
                })
            }
        }

        addEvents(element.children)
    }
}

document.addEventListener('click', (e) => {
    if(e.target.href) {
        const url = new URL(e.target.href)
        if(url.origin !== location.origin) return
        e.preventDefault()
        setPageTemplate(url.pathname)
    }
})

window.onpopstate = function (e) {
    setPageTemplate(window.location.pathname)
}

/**
 * @param {string} pathname
 */
function setPageTemplate(pathname) {    

    if(interval) clearInterval(interval)

    let url;

    try {

        let handler = getHandler(pathname)

        if(handler === '/') handler = ''

        url = `${handler}/HTML.js`
    
    } catch(err) {
        url = `/${err.cause}.js`
    }

    import(`/pages${url}`).then(async module => {
        window.history.replaceState({}, '', pathname)
        render = await module.default()
        interval = loopRender()
    })
}

/**
 * @param {string} pathname
 */
function getHandler(pathname) {

    let handler;

    if (pathname === '/') return pathname
    
    const paths = pathname.split('/').slice(1);

    let bestMatchKey = '';
    let bestMatchLength = -1;

    for (const [routeKey] of routes) {

        const routeSegs = routeKey.split('/')
        
        const isMatch = pathsMatch(routeSegs, paths.slice(0, routeSegs.length));

        if (isMatch && routeSegs.length > bestMatchLength) {
            bestMatchKey = routeKey;
            bestMatchLength = routeSegs.length;
        }
    }

    if (bestMatchKey) {

        handler = bestMatchKey

        params = parseParams(paths.slice(bestMatchLength))

        const slugMap = routes.get(bestMatchKey) ?? {}

        Object.entries(slugMap).forEach(([key, idx]) => {
            key = key.replace('[', '').replace(']', '')
            slugs[key] = paths[idx]
        })
    }

    if (!handler) throw new Error(`Route ${pathname} not found`, { cause: 404 });

    return handler
}

/** 
 * @param {string[]} routeSegs
 * @param {string[]} pathSegs
 */
function pathsMatch(routeSegs, pathSegs) {

    if (routeSegs.length !== pathSegs.length) {
        return false;
    }

    const slugs = routes.get(routeSegs.join('/')) || new Map()

    for (let i = 0; i < routeSegs.length; i++) {
        if (!slugs.has(routeSegs[i]) && routeSegs[i] !== pathSegs[i]) {
            return false;
        }
    }  

    return true;
}

/**
 * @param {string[]} input
 */
function parseParams(input) {

    const params = []

    for(const param of input) {

        const num = Number(param)

        if(!Number.isNaN(num)) params.push(num)

        else if(param === 'true') params.push(true)

        else if(param === 'false') params.push(false)

        else if(param === 'null') params.push(null)

        else if(param === 'undefined') params.push(undefined)

        else params.push(param)
    }

    return params
}