export function Logger() {

    const formatDate = () => new Date().toISOString().replace('T', ' ').replace('Z', '')

    const reset = '\x1b[0m'

    console.info = (...args: any[]) => {
        const info = `[${formatDate()}]\x1b[32m INFO${reset} (${process.pid})`
        console.log(info, ...args)
    }

    console.error = (...args: any[]) => {
        const err = `[${formatDate()}]\x1b[31m ERROR${reset} (${process.pid})`
        console.log(err, ...args)
    }

    console.debug = (...args: any[]) => {
        const bug = `[${formatDate()}]\x1b[36m DEBUG${reset} (${process.pid})`
        console.log(bug, ...args)
    }

    console.warn = (...args: any[]) => {
        const warn = `[${formatDate()}]\x1b[33m WARN${reset} (${process.pid})`
        console.log(warn, ...args)
    }

    console.trace = (...args: any[]) => {
        const trace = `[${formatDate()}]\x1b[35m TRACE${reset} (${process.pid})`
        console.log(trace, ...args)
    }
}