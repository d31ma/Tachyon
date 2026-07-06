// @ts-check
//
// Backend resolution coordinator.
//
// Yon ships the `server/` source as-is and runs every route as a process.
// There is no execution-backend registry any more (the in-house wasm backend
// has been removed; the Tac frontend worker compiler is separate): compiled
// languages compile on first request and cache the result, everything else
// runs through its interpreter or as an executable. These hooks remain for
// the server's `configureRoutes` lifecycle (they are safe to call
// repeatedly, e.g. on HMR) and as extension points for future backends.

/** Register per-handler execution backends. No-op: every route is a process. */
export function registerHandlerBackends() {}

/** Drop per-handler backend registrations (HMR reload). No-op. */
export function clearHandlerBackends() {}
