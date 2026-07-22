import { Minus, Square, Copy, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useMaximized } from "../useMaximized";

/**
 * Minimise / maximise / close for a window with no system decorations.
 *
 * The drag region itself lives on the header (`data-tauri-drag-region`);
 * these buttons sit inside it and keep their own clicks, since the drag only
 * fires when the press lands on the region rather than on a child.
 */
export default function WindowControls() {
  const maximized = useMaximized();
  const appWindow = getCurrentWindow();

  return (
    <div className="window-controls">
      <button
        className="window-button"
        onClick={() => appWindow.minimize()}
        title="Minimise"
        aria-label="Minimise"
      >
        <Minus size={15} />
      </button>
      <button
        className="window-button"
        onClick={() => appWindow.toggleMaximize()}
        title={maximized ? "Restore" : "Maximise"}
        aria-label={maximized ? "Restore" : "Maximise"}
      >
        {maximized ? <Copy size={12} /> : <Square size={12} />}
      </button>
      <button
        className="window-button close"
        onClick={() => appWindow.close()}
        title="Close"
        aria-label="Close"
      >
        <X size={15} />
      </button>
    </div>
  );
}
