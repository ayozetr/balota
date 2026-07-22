// SPDX-License-Identifier: GPL-3.0-or-later
import { History, Play } from "lucide-react";
import type { AppConfig } from "../types";
import { timeAgo } from "../format";

interface Props {
  config: AppConfig;
  onOpen: (id: string) => void;
}

export default function HistoryTab({ config, onOpen }: Props) {
  return (
    <div className="panel" style={{ flex: 1 }}>
      <div className="panel-head">
        <h2 style={{ fontSize: 14 }}>Recently played</h2>
        <span style={{ fontSize: 12, color: "var(--text-dim)" }}>
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
              style={{ gridTemplateColumns: "minmax(0, 1fr) 120px 130px 96px" }}
              onClick={() => onOpen(entry.id)}
            >
              <div style={{ minWidth: 0 }}>
                <div className="server-name">{entry.name}</div>
                <div className="server-addr mono">{entry.id}</div>
              </div>
              <div style={{ color: "var(--cyan)", fontSize: 12 }}>{entry.map}</div>
              <div style={{ fontSize: 12, color: "var(--text-muted)" }}>
                {timeAgo(entry.lastPlayed)}
                {entry.timesPlayed > 1 && ` · ${entry.timesPlayed}×`}
              </div>
              <button
                className="btn btn-sm"
                onClick={(e) => {
                  e.stopPropagation();
                  onOpen(entry.id);
                }}
              >
                <Play size={12} />
                Open
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
