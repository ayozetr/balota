// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect } from "react";

interface Shortcuts {
  onSearch: () => void;
  onPrevPage: () => void;
  onNextPage: () => void;
  onRefresh: () => void;
}

/**
 * Global keyboard shortcuts.
 *
 * Steam Deck's Game Mode hands a controller to desktop apps as keyboard input,
 * so the shoulder buttons land here as Page Up/Down. That makes paging through
 * a 50-row list possible without ever touching the trackpad.
 */
export function useShortcuts({
  onSearch,
  onPrevPage,
  onNextPage,
  onRefresh,
}: Shortcuts) {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable;

      // Escape gives the keyboard back to the list.
      if (e.key === "Escape" && typing) {
        target?.blur();
        return;
      }
      if (typing) return;

      switch (e.key) {
        case "/":
          e.preventDefault();
          onSearch();
          break;
        case "PageUp":
          e.preventDefault();
          onPrevPage();
          break;
        case "PageDown":
          e.preventDefault();
          onNextPage();
          break;
        case "F5":
          e.preventDefault();
          onRefresh();
          break;
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onSearch, onPrevPage, onNextPage, onRefresh]);
}
