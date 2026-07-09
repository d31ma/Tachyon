// @ts-check
//
// Fylo browser client entry point — re-exports from the sync engine which wraps
// Tachyon's vendored FYLO browser shim with HTTP sync, IndexedDB caching, and
// SSE subscriptions.
//
// The compiler auto-injects `import { fylo } from '../runtime/fylo-global.js'`
// when it detects bare `fylo` references in companion scripts. This shim
// preserves that import path while the implementation lives in fylo-browser-sync.js.
//
export { fylo, createFyloClient } from './fylo-browser-sync.js';
