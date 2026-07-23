// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useRef } from "react";

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

/**
 * Keeps Tab inside a dialog while it is open, and hands focus back to
 * whatever had it once the dialog closes.
 *
 * Without this, tabbing out of a modal lands on the list behind it: the focus
 * ring disappears under the dim layer and there is no way back except the
 * mouse — which on a Steam Deck means the trackpad.
 */
export function useFocusTrap<T extends HTMLElement>() {
  const ref = useRef<T>(null);

  useEffect(() => {
    const container = ref.current;
    if (!container) return;

    const previous = document.activeElement as HTMLElement | null;

    // Focus the first control rather than the container, so the first Tab
    // moves on instead of entering.
    const first = container.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? container).focus();

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;

      const items = Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE))
        .filter((el) => el.offsetParent !== null);
      if (items.length === 0) return;

      const firstItem = items[0];
      const lastItem = items[items.length - 1];
      const active = document.activeElement;

      // Wrap around at both ends.
      if (e.shiftKey && (active === firstItem || active === container)) {
        e.preventDefault();
        lastItem.focus();
      } else if (!e.shiftKey && active === lastItem) {
        e.preventDefault();
        firstItem.focus();
      }
    };

    container.addEventListener("keydown", onKeyDown);
    return () => {
      container.removeEventListener("keydown", onKeyDown);
      previous?.focus?.();
    };
  }, []);

  return ref;
}
