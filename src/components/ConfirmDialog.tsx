// SPDX-License-Identifier: GPL-3.0-or-later
import { AlertTriangle, X } from "lucide-react";
import { useFocusTrap } from "../useFocusTrap";
import { useEscape } from "../useEscape";

interface Props {
  title: string;
  /** What is about to happen, in the user's terms. */
  body: React.ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** Marks the action as irreversible: red button, warning icon. */
  destructive?: boolean;
}

/**
 * Confirmation for actions that cannot be undone.
 *
 * Cancel holds the initial focus on purpose. Enter is the pad's A button, and
 * this dialog exists precisely because deleting tens of gigabytes was one
 * accidental press away.
 */
export default function ConfirmDialog({
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
  destructive = false,
}: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  useEscape(onCancel);

  return (
    <div className="overlay" onClick={onCancel}>
      <div
        className="modal"
        ref={trapRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        tabIndex={-1}
        style={{ width: 480 }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3 id="confirm-title" style={{ flex: 1, fontSize: 15 }}>
            {destructive && (
              <AlertTriangle
                size={15}
                color="var(--red)"
                style={{ verticalAlign: -2, marginRight: 7 }}
              />
            )}
            {title}
          </h3>
          <button className="icon-btn" onClick={onCancel} aria-label="Close">
            <X size={17} />
          </button>
        </div>

        <div className="modal-body">{body}</div>

        <div className="modal-foot">
          <div style={{ flex: 1 }} />
          <button className="btn" onClick={onCancel} data-autofocus>
            Cancel
          </button>
          <button
            className={`btn${destructive ? " btn-danger" : " btn-primary"}`}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
