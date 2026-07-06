// @ts-check
//
// Fylo browser client entry point — re-exports from the sync engine which wraps
// the official @d31ma/fylo/browser module with HTTP sync, IndexedDB caching, and
// SSE subscriptions.
//
// The compiler auto-injects `import { fylo } from '../runtime/fylo-global.js'`
// when it detects bare `fylo` references in companion scripts. This shim
// preserves that import path while the implementation lives in fylo-browser-sync.js.
//
// Deprecated modules (kept for one release, not imported at runtime):
//   fylo-local.js — replaced by @d31ma/fylo/browser OPFS storage
//   fylo-opfs-fs.js — built into @d31ma/fylo/browser, never wired into runtime

export { fylo, createFyloClient } from './fylo-browser-sync.js';
