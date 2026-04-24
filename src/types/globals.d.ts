declare module '/modules/*' {
  const value: any;
  export default value;
}

declare module '*.css' {
  const value: string;
  export default value;
}

declare global {
  var __ty_prerender__: boolean | undefined;
  const Tac: typeof import("../runtime/tac.js").default;
  type TacProps = import("../runtime/tac.js").TacProps;

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
  }
}

export {};
