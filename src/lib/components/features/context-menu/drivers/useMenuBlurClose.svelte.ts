import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const SUPPRESSED_BLUR_RECHECK_MS = 150;

type Opts = {
  isInBlurGrace: () => boolean;
  isSuppressed: () => boolean;
  isCapturing: () => boolean;
  resumeClose: () => void;
  closeMenu: () => Promise<void> | void;
};

export function useMenuBlurClose(opts: Opts) {
  let suppressedBlurCheckTimer: ReturnType<typeof setTimeout> | null = null;
  let unlistenFocus: (() => void) | undefined;

  async function init() {
    const win = getCurrentWebviewWindow();
    unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        if (suppressedBlurCheckTimer) {
          clearTimeout(suppressedBlurCheckTimer);
          suppressedBlurCheckTimer = null;
        }
        return;
      }
      if (opts.isInBlurGrace()) return;
      if (opts.isCapturing()) return;
      if (opts.isSuppressed()) {
        opts.resumeClose();
        if (suppressedBlurCheckTimer) clearTimeout(suppressedBlurCheckTimer);
        suppressedBlurCheckTimer = setTimeout(() => {
          suppressedBlurCheckTimer = null;
          win.isFocused()
            .then((stillFocused) => {
              if (!stillFocused) void opts.closeMenu();
            })
            .catch(() => {});
        }, SUPPRESSED_BLUR_RECHECK_MS);
        return;
      }
      void opts.closeMenu();
    });
  }

  function destroy() {
    if (suppressedBlurCheckTimer) {
      clearTimeout(suppressedBlurCheckTimer);
      suppressedBlurCheckTimer = null;
    }
    unlistenFocus?.();
    unlistenFocus = undefined;
  }

  return { init, destroy };
}

export type MenuBlurClose = ReturnType<typeof useMenuBlurClose>;
