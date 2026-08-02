// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect } from "react";

/**
 * Closes a dialog on Escape.
 *
 * Every dialog needs this and only one of them used to have it, so "Direct
 * connect" could not be closed with Escape — or with B on a controller, which
 * is the same key by the time it reaches the page — despite the README and the
 * on-screen controller help both saying it could.
 */
export function useEscape(onEscape: () => void) {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onEscape();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onEscape]);
}
