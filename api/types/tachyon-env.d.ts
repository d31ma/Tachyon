declare module '/shared/modules/*' {
  const value: any;
  export default value;
}

declare module '*.css' {
  const value: string;
  export default value;
}

// Material Web Components — loaded at runtime via importmap from esm.sh in
// application-owned admin shells. Tachyon does not bundle the package itself;
// these ambient declarations satisfy the typechecker for side-effect imports.
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
  var __tc_prerender__: boolean | undefined;
  const Tac: any;
  type TacProps = any;
  const env: any;
  const onMount: any;
  const publish: any;
  const subscribe: any;
  type TacPlatformContext = any;
  const platform: TacPlatformContext['platform'];
  const environment: TacPlatformContext['environment'];
  const os: TacPlatformContext['os'];
  const target: TacPlatformContext['target'];
  /** Implicit portable platform prelude available in JS/TS companion scripts. */
  const app: {
    isAvailable(): boolean;
    info(): Promise<Record<string, unknown>>;
  };
  const clipboard: {
    readText(): Promise<string>;
    writeText(text: string): Promise<unknown>;
  };
  const fileSystem: {
    readText(path: string): Promise<{ path: string; text: string }>;
    writeText(path: string, text: string): Promise<{ path: string; bytes: number; written: boolean }>;
    readDir(path: string): Promise<{ path: string; entries: Array<{ name: string; type: 'file' | 'directory' }> }>;
    stat(path: string): Promise<{ path: string; exists: boolean; type?: 'file' | 'directory'; size?: number }>;
    mkdir(path: string): Promise<{ path: string; created: boolean }>;
    remove(path: string): Promise<{ path: string; removed: boolean }>;
    paths(): Promise<{ appData: string; cache: string; documents?: string }>;
  };
  const shell: {
    exec(command: string, args?: string[], cwd?: string): Promise<{ command: string; args: string[]; exitCode: number; stdout: string; stderr: string }>;
  };
  const browser: {
    open(url: string): Promise<{ opened: boolean }>;
  };
  const share: {
    text(text: string, title?: string): Promise<{ shared: boolean }>;
  };
  const haptics: {
    impact(): Promise<{ impacted: boolean }>;
  };
  const filePicker: {
    openText(): Promise<{ name: string; text: string }>;
    saveText(name: string, text: string): Promise<{ name: string; saved: boolean }>;
  };
  const secrets: {
    get(key: string): Promise<string | null>;
    set(key: string, value: string): Promise<void>;
    delete(key: string): Promise<void>;
  };
  const auth: {
    verifyUser(reason: string): Promise<{ verified: boolean; method: 'biometric' | 'device-credential' }>;
  };
  const geolocation: {
    current(options?: PositionOptions): Promise<{ latitude: number; longitude: number; accuracy: number; altitude: number | null; altitudeAccuracy: number | null; heading: number | null; speed: number | null; timestamp: number }>;
  };
  const notifications: {
    show(title: string, options?: NotificationOptions): Promise<{ shown: boolean }>;
  };
  const media: {
    getUserMedia(constraints: MediaStreamConstraints): Promise<MediaStream>;
  };
  interface TachyonShortcut {
    id: string;
    accelerator: string;
  }
  interface TachyonShortcutListResult {
    shortcuts: TachyonShortcut[];
  }
  const host: {
    invoke<T = unknown>(operation: string, payload?: unknown): Promise<T>;
    on<T = unknown>(event: string, handler: (payload: T) => void): () => void;
  };
  const shortcuts: {
    register(options: { id: string; accelerator: string; replace?: boolean }): Promise<TachyonShortcutListResult & { shortcut: TachyonShortcut }>;
    unregister(id: string): Promise<TachyonShortcutListResult & { unregistered: boolean }>;
    unregisterAll(): Promise<TachyonShortcutListResult & { unregistered: number }>;
    list(): Promise<TachyonShortcutListResult>;
  };
  interface TachyonAppWindowState {
    mode?: 'normal' | 'minimized' | 'maximized' | 'fullscreen';
    alwaysOnTop: boolean;
    opacity: number;
    clickThrough?: boolean;
    captureProtection?: boolean;
  }
  const appWindow: {
    state(): Promise<TachyonAppWindowState>;
    setAlwaysOnTop(enabled: boolean): Promise<TachyonAppWindowState | { updated: true }>;
    setOpacity(value: number): Promise<TachyonAppWindowState | { updated: true }>;
    setClickThrough(enabled: boolean): Promise<TachyonAppWindowState>;
    setCaptureProtection(enabled: boolean): Promise<TachyonAppWindowState>;
  };
  interface TachyonContentSurfaceState {
    id: string;
    open: boolean;
    presentation?: 'composed' | 'detached';
    persistentSession?: boolean;
    url?: string;
    loading?: boolean;
    canGoBack?: boolean;
    canGoForward?: boolean;
  }
  const contentSurface: {
    open(options: { id: string; url: string; persistentSession?: boolean }): Promise<TachyonContentSurfaceState>;
    navigate(id: string, url: string): Promise<TachyonContentSurfaceState>;
    state(id: string): Promise<TachyonContentSurfaceState>;
    goBack(id: string): Promise<TachyonContentSurfaceState>;
    goForward(id: string): Promise<TachyonContentSurfaceState>;
    reload(id: string): Promise<TachyonContentSurfaceState>;
    close(id: string): Promise<TachyonContentSurfaceState & { open: false }>;
  };
  interface TachyonCaptureWindow {
    windowId: string;
    title: string;
    application: string;
    frame: { x: number; y: number; width: number; height: number };
  }
  interface TachyonCaptureResult {
    windowId: string;
    destination: 'clipboard' | 'file' | 'both';
    format: 'png';
    bytes: number;
    clipboard: boolean;
    path: string;
  }
  const screenCapture: {
    state(): Promise<{ supported: boolean; permission: 'granted' | 'prompt' | 'denied' | 'unsupported'; format: 'png'; destinations: Array<'clipboard' | 'file' | 'both'> }>;
    listWindows(options?: { visibleOnly?: boolean; excludeCurrentApp?: boolean }): Promise<{ windows: TachyonCaptureWindow[]; permission: 'granted' | 'prompt' | 'unsupported' }>;
    captureWindow(options: { windowId: string; destination: 'clipboard' | 'file' | 'both'; format?: 'png' }): Promise<TachyonCaptureResult>;
  };
  const capabilities: {
    supports(capability: string): boolean;
    state(capability: string): Promise<'granted' | 'denied' | 'prompt' | 'unsupported'>;
  };
  interface Window {
    __tc_fetch_cache_db__?: IDBDatabase | null;
    __tc_onMount_flushed__?: boolean;
    __tc_onMount_queue__?: Array<() => void | Promise<void>>;
    __tc_public_env__?: Record<string, unknown>;
    __tc_rerender?: (componentRootId?: string) => void;
    /**
     * HMR (dev only) — targeted re-import of the changed module paths,
     * re-rendering the current view in place. Installed by spa-renderer.js.
     */
    __tachyon_hmr_update__?: (paths: string[]) => Promise<void>;
    /**
     * HMR (dev only) — full in-place soft reload: clears the Tac module
     * cache and re-navigates. Installed by spa-renderer.js.
     */
    __tachyon_hmr_reload__?: () => Promise<void>;
    __tc_signals__?: {
      values: Map<string, unknown>;
      listeners: Map<string, Set<(value: unknown) => void | Promise<void>>>;
    };
    __tcNativeBridge__?: {
      version: number;
      postMessage: (message: unknown) => boolean;
      invoke: (capability: string, payload?: unknown, options?: { source?: string; timeoutMs?: number }) => Promise<unknown>;
      onMessage: (handler: (message: unknown) => void) => () => void;
      messageHandler?: (message: unknown) => void;
    };
    Tac?: {
      version?: string;
      modules?: Map<string, unknown>;
      platform?: TacPlatformContext;
      register: (path: string, factory: unknown) => unknown;
      load: (path: string) => Promise<unknown>;
    };
  }

  var __tcNativeCapabilities__: Record<string, (payload: unknown, request?: unknown) => unknown | Promise<unknown>> | undefined;
}

export {};
