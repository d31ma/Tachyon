/// <reference path="../api/types/tachyon-env.d.ts" />

interface HTMLElement {
  tachyonIsland?: {
    refresh?: () => void | Promise<void>;
  };
}
