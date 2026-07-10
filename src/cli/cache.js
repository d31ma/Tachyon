// @ts-check
import TachyonRuntimeCache from '../shared/runtime-cache.js';

/** @param {string[]} [args] */
export async function runCacheCommand(args = process.argv.slice(2)) {
    const action = args[0];
    if (!action || action === 'status') {
        const status = await TachyonRuntimeCache.status();
        console.log(`Tachyon cache\n  Root: ${status.root}\n  Runtime entries: ${status.entries}`);
        return;
    }
    if (action === 'clean') {
        await TachyonRuntimeCache.clear();
        console.log(`Cleared Tachyon runtime cache: ${TachyonRuntimeCache.runtimeRoot()}`);
        return;
    }
    throw new Error(`Unknown cache command '${action}'. Use: ty cache [status|clean]`);
}

if (import.meta.main)
    await runCacheCommand();
