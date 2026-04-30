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
  const inject: typeof import("../runtime/decorators.js").inject;
  const provide: typeof import("../runtime/decorators.js").provide;
  const env: typeof import("../runtime/decorators.js").env;
  const onMount: typeof import("../runtime/decorators.js").onMount;
  const emit: typeof import("../runtime/decorators.js").emit;
  const render: typeof import("../runtime/decorators.js").render;
  /**
   * FYLO collection query helper — globally available in Tac companion scripts
   * and on `window` for plain script tags. Bootstrapped by imports.js.
   *
   * Usage:
   *   await fylo.users.find({ $ops: [{ role: { $eq: 'admin' } }] })
   *   await fylo.users.get('usr_xxx')
   *   await fylo.users.patch('usr_xxx', { role: 'admin' })
   *   await fylo.users.del('usr_xxx')
   *   await fylo.sql('SELECT * FROM users LIMIT 10')
   *   await fylo.collections()
   *
   * Reserved property names (not usable as collection names): enabled, root,
   * sql, collections.
   */
  const fylo: FyloApi;

  interface Window {
    __ty_context__?: Map<string, unknown>;
    __ty_fetch_cache_db__?: IDBDatabase | null;
    __ty_onMount_queue__?: Array<() => void | Promise<void>>;
    __ty_public_env__?: Record<string, unknown>;
    __ty_rerender?: () => void;
    Tac?: {
      version?: string;
      modules?: Map<string, unknown>;
      register: (path: string, factory: unknown) => unknown;
      load: (path: string) => Promise<unknown>;
    };
    /**
     * Global FYLO client. Same as the global `fylo` — populated by
     * browser/shared/scripts/imports.js. Property access returns a
     * per-collection proxy; sql/collections live as own properties.
     */
    fylo?: FyloApi;
  }

  /**
   * Per-collection proxy returned by `fylo.<collectionName>` property access.
   * All methods are async and return JSON envelopes from the /_fylo/api/* server.
   */
  interface FyloCollectionProxy {
    find(query?: Record<string, unknown>): Promise<FyloQueryResult>;
    get(id: string): Promise<FyloDocResponse>;
    patch(id: string, doc: Record<string, unknown>): Promise<{ ok?: boolean; id?: string; error?: string }>;
    del(id: string): Promise<{ ok?: boolean; error?: string }>;
  }

  /**
   * Reserved (non-collection) properties on the FYLO global. Backed by a Proxy
   * — any other string property access returns a `FyloCollectionProxy`.
   */
  interface FyloApiCommands {
    enabled: boolean;
    root?: string;
    sql(source: string): Promise<FyloQueryResult>;
    collections(): Promise<FyloCollectionsResponse>;
  }

  /**
   * Property-access global. `fylo.users` returns a per-collection proxy;
   * reserved keys (`sql`, `collections`, `enabled`, `root`) keep their
   * specific types.
   */
  type FyloApi = FyloApiCommands & { readonly [collection: string]: FyloCollectionProxy };
}
/**
 * @typedef {Object} FyloQueryResult
 * @property {string} [error]
 * @property {string} kind - "sql" or "find"
 * @property {Array<{ id: string, doc: unknown }>} [docs] - For find results
 * @property {unknown} [result] - For SQL results
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
 * @property {Array<{ id: string, data: Record<string, unknown>, createdAt: string | number, updatedAt: string | number, isHead?: boolean, deleted?: boolean }>} [history]
 * @property {string} [docError]
 * @property {string} [historyError]
 * @property {string[]} [encryptedFields]
 * @property {boolean} [revealed]
 */

export {};
