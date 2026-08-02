// @ts-check
//
// GET/HEAD /language — the JavaScript face of the consolidated polyglot route.
// The same route path is also served by yon.rs (POST, DELETE) and yon.cpp
// (PUT, PATCH), demonstrating one Yon route split across three languages.

export class Handler {
  /** @returns {Record<string, unknown>} */
  static GET() {
    return {
      route: '/language',
      description: 'One route, three languages: JavaScript answers GET/HEAD, Rust answers POST/DELETE, C++ answers PUT/PATCH.',
      methods: {
        GET: 'JavaScript route map (this response)',
        HEAD: 'JavaScript headers-only probe',
        POST: 'Rust echo handler compiled in-house to WebAssembly',
        DELETE: 'Rust size-confirm handler compiled in-house to WebAssembly',
        PUT: 'C++ echo handler compiled in-house to WebAssembly',
        PATCH: 'C++ patch-summary handler compiled in-house to WebAssembly',
      },
      routes: [
        { path: '/language', purpose: 'polyglot method split across JavaScript, Rust and C++' },
        { path: '/language/javascript', purpose: 'status-code diagnostics and echo responses' },
        { path: '/language/python', purpose: 'Python GET handler' },
        { path: '/language/rust', purpose: 'Rust GET handler' },
        { path: '/language/items', purpose: 'RESTful CRUD in TypeScript' },
        { path: '/language/items/:id', purpose: 'single-item CRUD in TypeScript' },
        { path: '/language/versions/:version', purpose: 'dynamic path parameter in Python' },
        { path: '/language/fylo', purpose: 'FYLO machine-interface showcase' },
        { path: '/language/telemetry', purpose: 'OTLP trace summary from FYLO' },
      ],
    }
  }

  /** @returns {Record<string, never>} */
  static HEAD() {
    return {}
  }
}
