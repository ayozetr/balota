// SPDX-License-Identifier: GPL-3.0-or-later
import { History, Play, X } from "lucide-react";
import type { AppConfig } from "../types";
import { timeAgo } from "../format";

interface Props {
  config: AppConfig;
  onOpen: (id: string) => void;
  onForget: (id: string) => void;
}

export default function HistoryTab({ config, onOpen, onForget }: Props) {
  return (
    <div className="panel" style={{ flex: 1 }}>
      <div className="panel-head">
        <h2 style={{ fontSize: 14 }}>Recently played</h2>
        <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
          {config.history.length} server{config.history.length === 1 ? "" : "s"}
        </span>
      </div>

      <div className="rows">
        {config.history.length === 0 ? (
          <div className="empty">
            <History size={22} />
            Nothing here yet. Servers you join show up in this list.
          </div>
        ) : (
          config.history.map((entry) => (
            <div
              key={entry.id}
              className="row row-item"
              style={{ gridTemplateColumns: "minmax(0, 1fr) 120px 130px 96px 34px" }}
              role="button"
              tabIndex={0}
              aria-label={`${entry.name}, last played ${timeAgo(entry.lastPlayed)}`}
              onClick={() => onOpen(entry.id)}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget) return;
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onOpen(entry.id);
                }
              }}
            >
              <div style={{ minWidth: 0 }}>
                <div className="server-name clamp">{entry.name}</div>
                <div className="server-addr mono clamp">{entry.id}</div>
              </div>
              <div style={{ color: "var(--cyan)", fontSize: 12 }} className="clamp">
                {entry.map}
              </div>
              <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
                {timeAgo(entry.lastPlayed)}
                {entry.timesPlayed > 1 && ` · ${entry.timesPlayed}×`}
              </div>
              <button
                className="btn btn-sm"
                tabIndex={-1}
                onClick={(e) => {
                  e.stopPropagation();
                  onOpen(entry.id);
                }}
              >
                <Play size={12} />
                Open
              </button>
              {/* A server that no longer exists otherwise sits here forever,
                  opening a panel that fails and closes itself. */}
              <button
                className="icon-btn"
                tabIndex={-1}
                title="Remove from history"
                aria-label={`Remove ${entry.name} from history`}
                onClick={(e) => {
                  e.stopPropagation();
                  onForget(entry.id);
                }}
              >
                <X size={14} />
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
