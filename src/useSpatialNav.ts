// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect } from "react";

const FOCUSABLE = [
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
  ".row-item",
].join(",");

interface Rect {
  el: HTMLElement;
  x: number;
  y: number;
}

function centre(el: HTMLElement): Rect | null {
  const r = el.getBoundingClientRect();
  if (r.width === 0 || r.height === 0) return null;
  return { el, x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/**
 * Directional navigation for the whole window.
 *
 * Arrow keys only moved inside the server table, so a controller could never
 * reach the sidebar, the filters or the pager: everywhere else expects Tab,
 * and a d-pad has no Tab. This picks the nearest focusable element in the
 * direction pressed, the way a console interface behaves.
 *
 * It runs only when nothing else has handled the key, so the table keeps its
 * own up/down behaviour and dialogs keep their focus trap.
 */
export function useSpatialNav() {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      if (!["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) return;

      const active = document.activeElement as HTMLElement | null;
      const typing =
        active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement;
      // Let the caret move inside a text field.
      if (typing && (e.key === "ArrowLeft" || e.key === "ArrowRight")) return;

      // A dialog keeps navigation to itself.
      const scope =
        active?.closest<HTMLElement>('[role="dialog"]') ??
        document.querySelector<HTMLElement>(".app");
      if (!scope) return;

      const from = active && scope.contains(active) ? centre(active) : null;
      const candidates = Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE))
        .filter((el) => el !== active && el.offsetParent !== null)
        .map(centre)
        .filter((c): c is Rect => c !== null);

      if (candidates.length === 0) return;

      // Nothing focused yet: start at the top-left of the scope.
      if (!from) {
        e.preventDefault();
        candidates.sort((a, b) => a.y - b.y || a.x - b.x)[0].el.focus();
        return;
      }

      const horizontal = e.key === "ArrowLeft" || e.key === "ArrowRight";
      const sign = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : -1;

      let best: Rect | null = null;
      let bestScore = Infinity;

      for (const c of candidates) {
        const along = horizontal ? (c.x - from.x) * sign : (c.y - from.y) * sign;
        // Must actually lie in the direction pressed, with a small threshold
        // so items on the same row do not count as "below".
        if (along < 8) continue;

        const across = horizontal ? Math.abs(c.y - from.y) : Math.abs(c.x - from.x);
        // Distance along the axis matters, drift across it matters more —
        // otherwise a far-away item that happens to be aligned wins.
        const score = along + across * 2.5;
        if (score < bestScore) {
          bestScore = score;
          best = c;
        }
      }

      if (best) {
        e.preventDefault();
        best.el.focus();
        best.el.scrollIntoView({ block: "nearest", inline: "nearest" });
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
