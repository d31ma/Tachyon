declare module '/modules/*' {
  const value: any;
  export default value;
}

declare module '*.css' {
  const value: string;
  export default value;
}

// Material Web Components — loaded at runtime via importmap from esm.sh in
// the /_fylo and /api-docs admin shells. Tachyon does not bundle the package
// itself; these ambient declarations satisfy the typechecker for side-effect
// imports.
declare module '@material/web/chips/assist-chip.js';
declare module '@material/web/list/list.js';
declare module '@material/web/list/list-item.js';
declare module '@material/web/button/text-button.js';
declare module '@material/web/button/filled-button.js';
declare module '@material/web/button/outlined-button.js';
declare module '@material/web/iconbutton/icon-button.js';
declare module '@material/web/icon/icon.js';
declare module '@material/web/divider/divider.js';
declare module '@material/web/textfield/outlined-text-field.js';
declare module '@material/web/textfield/filled-text-field.js';
declare module '@material/web/chips/chip-set.js';
declare module '@material/web/chips/suggestion-chip.js';
declare module '@material/web/elevation/elevation.js';
declare module '@material/web/progress/circular-progress.js';

declare global {
  var __ty_prerender__: boolean | undefined;
  const Tac: typeof import("../runtime/tac.js").default;
  type TacProps = import("../runtime/tac.js").TacProps;
  const env: typeof import("../runtime/decorators.js").env;
  const onMount: typeof import("../runtime/decorators.js").onMount;
  const publish: typeof import("../runtime/decorators.js").publish;
  const subscribe: typeof import("../runtime/decorators.js").subscribe;
  type Json = unknown;
  interface TacWorkerRequest {
    len(): number;
    body(): string;
    json(): Json;
  }
  function json(value: string | Json): Json;
  /**
   * FYLO collection query helper — globally available in Tac companion scripts
   * and on `window` for plain script tags. Bootstrapped by
   * `src/runtime/fylo-global.js`, which self-creates `window.fylo` from the
   * shell-injected `<meta name="fylo-browser-path">` tag.
   *
   * Usage:
   *   await fylo.users.find({ $ops: [{ role: { $eq: 'admin' } }] })
   *   await fylo.users.get('usr_xxx')
   *   await fylo.users.patch('usr_xxx', { role: 'admin' })
   *   await fylo.users.del('usr_xxx')
   *   await fylo.sql('SELECT * FROM users LIMIT 10')
   *   await fylo.collections()
   *   fylo.setCredentials('user', 'pass')
   *   fylo.clearCredentials()
   *
   * Reserved property names (not usable as collection names): enabled, root,
   * sql, collections, setCredentials, clearCredentials, meta.
   */
  const fylo: FyloApi;

  interface Window {
    __ty_fetch_cache_db__?: IDBDatabase | null;
    __ty_onMount_queue__?: Array<() => void | Promise<void>>;
    __ty_public_env__?: Record<string, unknown>;
    __ty_rerender?: () => void;
    __ty_signals__?: {
      values: Map<string, unknown>;
      listeners: Map<string, Set<(value: unknown) => void | Promise<void>>>;
    };
    Tac?: {
      version?: string;
      modules?: Map<string, unknown>;
      register: (path: string, factory: unknown) => unknown;
      load: (path: string) => Promise<unknown>;
    };
    /**
     * Global FYLO client. Same as the global `fylo` — populated by
     * src/runtime/fylo-global.js. Property access returns a
     * per-collection proxy; collections/meta live as own properties.
     */
    fylo?: FyloApi;
  }

  /**
   * Per-collection proxy returned by `fylo.<collectionName>` property access.
   * All methods are async and return JSON envelopes from the /_fylo/api/* server.
   */
  type FyloCachePolicy = 'cache-first' | 'network-first' | 'reload' | 'no-store';
  interface FyloQueryOptions {
    cache?: FyloCachePolicy;
  }

  type FyloSubscribeSource = 'initial' | 'event-stream' | 'poll' | 'local';
  interface FyloSubscribeMeta {
    collection: string;
    events: unknown[];
    offset: number;
    source: FyloSubscribeSource;
  }
  type FyloSubscribeCallback = (payload: FyloQueryResult, meta: FyloSubscribeMeta) => void | Promise<void>;
  type FyloSubscribeOptions = FyloQueryOptions & {
    pollMs?: number;
    since?: number;
    onError?: (error: unknown) => void;
  };

  interface FyloCollectionProxy {
    /**
     * Query the collection using PostgREST-style filters.
     * Values follow `operator.value` syntax: `{ role: 'eq.admin', age: 'gt.18' }`.
     * Reserved keys: `select` (vertical filter), `order` (sort), `limit`, `offset`.
     */
    find(query?: Record<string, unknown>, options?: FyloQueryOptions): Promise<FyloQueryResult>;
    list(limit?: number, options?: FyloQueryOptions): Promise<{ docs: Array<{ id: string; doc: unknown }>; error?: string; encryptedFields?: string[]; revealed?: boolean }>;
    get(id: string, options?: FyloQueryOptions): Promise<FyloDocResponse>;
    events(since?: number): Promise<{ collection: string; events: unknown[]; offset: number; exists: boolean; error?: string }>;
    subscribe(callback: FyloSubscribeCallback, options?: FyloSubscribeOptions): () => void;
    subscribe(query: Record<string, unknown>, callback: FyloSubscribeCallback, options?: FyloSubscribeOptions): () => void;
    create(doc: Record<string, unknown>): Promise<{ ok?: boolean; id?: string; doc?: unknown; error?: string }>;
    put(id: string, doc: Record<string, unknown>): Promise<{ ok?: boolean; id?: string; error?: string }>;
    patch(id: string, doc: Record<string, unknown>): Promise<{ ok?: boolean; id?: string; error?: string }>;
    del(id: string): Promise<{ ok?: boolean; error?: string }>;
    rebuild(): Promise<{ ok?: boolean; result?: unknown; error?: string }>;
  }

  /**
   * Reserved (non-collection) properties on the FYLO global. Backed by a Proxy
   * — any other string property access returns a `FyloCollectionProxy`.
   */
  interface FyloApiCommands {
    enabled: boolean;
    root?: string;
    setCredentials(user: string, pass: string): void;
    clearCredentials(): void;
    collections(): Promise<FyloCollectionsResponse>;
    meta(): Promise<{ root: string; readOnly: boolean; revealed: boolean; path: string } | null>;
  }

  /**
   * Property-access global. `fylo.users` returns a per-collection proxy;
   * reserved keys (`collections`, `enabled`, `root`) keep their
   * specific types.
   */
  type FyloApi = FyloApiCommands & { readonly [collection: string]: FyloCollectionProxy };
}
/**
 * @typedef {Object} FyloQueryResult
 * @property {string} [error]
 * @property {Array<{ id: string, doc: unknown }>} [docs]
 * @property {string} [collection]
 * @property {string[]} [encryptedFields]
 * @property {boolean} [revealed]
 */
/**
 * @typedef {Object} FyloCollectionsResponse
 * @property {string} root
 * @property {Array<{ name: string, exists: boolean, docsStored?: number, indexedDocs?: number, worm?: boolean, error?: string }>} collections
 */
/**
 * @typedef {Object} FyloDocResponse
 * @property {Record<string, unknown>} [doc]
 * @property {string} [docError]
 * @property {string[]} [encryptedFields]
 * @property {boolean} [revealed]
 */

export {};
