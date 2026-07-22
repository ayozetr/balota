// SPDX-License-Identifier: GPL-3.0-or-later
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Invisible grab strips around the window.
 *
 * Without system decorations the compositor no longer provides resize
 * handles, so the window would be stuck at its starting size. These strips
 * hand the gesture back to it.
 */
const EDGES = [
  { dir: "North", style: { top: 0, left: 6, right: 6, height: 5, cursor: "ns-resize" } },
  { dir: "South", style: { bottom: 0, left: 6, right: 6, height: 5, cursor: "ns-resize" } },
  { dir: "West", style: { left: 0, top: 6, bottom: 6, width: 5, cursor: "ew-resize" } },
  { dir: "East", style: { right: 0, top: 6, bottom: 6, width: 5, cursor: "ew-resize" } },
  { dir: "NorthWest", style: { top: 0, left: 0, width: 8, height: 8, cursor: "nwse-resize" } },
  { dir: "NorthEast", style: { top: 0, right: 0, width: 8, height: 8, cursor: "nesw-resize" } },
  { dir: "SouthWest", style: { bottom: 0, left: 0, width: 8, height: 8, cursor: "nesw-resize" } },
  { dir: "SouthEast", style: { bottom: 0, right: 0, width: 8, height: 8, cursor: "nwse-resize" } },
] as const;

export default function ResizeEdges() {
  return (
    <>
      {EDGES.map(({ dir, style }) => (
        <div
          key={dir}
          className="resize-edge"
          style={style}
          onMouseDown={(e) => {
            // Left button only: a right-click here should fall through.
            if (e.button !== 0) return;
            e.preventDefault();
            void getCurrentWindow().startResizeDragging(dir);
          }}
        />
      ))}
    </>
  );
}
