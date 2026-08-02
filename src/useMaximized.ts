// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Tracks whether the window is maximised.
 *
 * It matters for more than the button icon: a maximised window must drop its
 * rounded corners and its shadow margin, or the screen edges show transparent
 * gaps where the desktop shines through.
 */
export function useMaximized(): boolean {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let mounted = true;

    const sync = () =>
      appWindow.isMaximized().then(setMaximized).catch(() => undefined);

    void sync();
    // The compositor can maximise the window too — edge snapping, a keyboard
    // shortcut — so the state has to follow the window, not just our button.
    appWindow
      .onResized(sync)
      .then((fn) => {
        // Unmounting before this resolved used to leave the listener attached
        // with nothing left to detach it.
        if (mounted) unlisten = fn;
        else fn();
      })
      .catch(() => undefined);

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  return maximized;
}
