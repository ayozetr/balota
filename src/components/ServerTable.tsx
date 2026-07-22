// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Loader2,
  Lock,
  Play,
  ServerOff,
  Star,
} from "lucide-react";
import type { PingResult, ServerPage, ServerRow, SortMode } from "../types";
import { formatNumber, pingClass } from "../format";

interface Props {
  page: ServerPage | null;
  loading: boolean;
  favorites: string[];
  pings: Record<string, PingResult>;
  selectedId: string | null;
  sort: SortMode;
  onSort: (sort: SortMode) => void;
  onSelect: (server: ServerRow) => void;
  onToggleFavorite: (id: string) => void;
  onJoin: (server: ServerRow) => void;
  onPage: (page: number) => void;
  emptyMessage: string;
}

export default function ServerTable({
  page,
  loading,
  favorites,
  pings,
  selectedId,
  sort,
  onSort,
  onSelect,
  onToggleFavorite,
  onJoin,
  onPage,
  emptyMessage,
}: Props) {
  const rows = page?.items ?? [];

  // Ping is measured here rather than served by the API, so sorting by it can
  // only order what has already been probed: this page. Unmeasured and
  // unreachable servers sink to the bottom instead of pretending to be fast.
  const items =
    sort === "ping"
      ? [...rows].sort((a, b) => {
          const pa = pings[a.id];
          const pb = pings[b.id];
          const va = pa?.online ? (pa.pingMs ?? Infinity) : Infinity;
          const vb = pb?.online ? (pb.pingMs ?? Infinity) : Infinity;
          return va - vb || b.players - a.players;
        })
      : rows;

  const [focusIndex, setFocusIndex] = useState(0);

  // Keep the roving index inside the list when the page changes under it.
  useEffect(() => {
    setFocusIndex((i) => Math.min(i, Math.max(0, items.length - 1)));
  }, [items.length]);

  function focusRow(container: HTMLElement, index: number) {
    const rows = Array.from(
      container.querySelectorAll<HTMLElement>(".row-item"),
    );
    const row = rows[Math.max(0, Math.min(rows.length - 1, index))];
    row?.focus();
    row?.scrollIntoView({ block: "nearest" });
  }

  /** Arrow keys move, Enter opens, and the row's own buttons get a letter
   *  each, since they are out of the tab order. */
  function move(e: React.KeyboardEvent, index: number, server: ServerRow) {
    const list = e.currentTarget.parentElement;
    if (!list) return;

    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp":
        e.preventDefault();
        focusRow(list as HTMLElement, index + (e.key === "ArrowDown" ? 1 : -1));
        break;
      case "Home":
        e.preventDefault();
        focusRow(list as HTMLElement, 0);
        break;
      case "End":
        e.preventDefault();
        focusRow(list as HTMLElement, items.length - 1);
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        onSelect(server);
        break;
      case "f":
      case "F":
        e.preventDefault();
        onToggleFavorite(server.id);
        break;
      case "j":
      case "J":
        e.preventDefault();
        onJoin(server);
        break;
    }
  }

  const total = page?.total ?? 0;
  const pageSize = page?.pageSize ?? 50;
  const current = page?.page ?? 0;
  const lastPage = Math.max(0, Math.ceil(total / pageSize) - 1);

  return (
    <div className="panel table">
      <div className="row row-head">
        <span />
        <button onClick={() => onSort("name")} title="Sort by name">
          Server {sort === "name" && "▾"}
        </button>
        <button onClick={() => onSort("map")} title="Sort by map">
          Map {sort === "map" && "▾"}
        </button>
        <button
          className="num"
          onClick={() => onSort("players")}
          title="Sort by players"
        >
          Players {sort === "players" && "▾"}
        </button>
        <button className="num" onClick={() => onSort("ping")} title="Sort by ping">
          Ping {sort === "ping" && "▾"}
        </button>
        <span>Mods</span>
        <span />
      </div>

      <div className="rows">
        {loading && items.length === 0 ? (
          <div className="empty">
            <Loader2 size={22} className="spin" />
            Loading servers…
          </div>
        ) : items.length === 0 ? (
          <div className="empty">
            <ServerOff size={22} />
            {emptyMessage}
          </div>
        ) : (
          items.map((server, index) => {
            const ping = pings[server.id];
            const isFavorite = favorites.includes(server.id);

            return (
              <div
                key={server.id}
                className={`row row-item${selectedId === server.id ? " selected" : ""}`}
                onClick={() => onSelect(server)}
                role="button"
                aria-label={`${server.name}, ${server.players} of ${server.maxPlayers} players`}
                // Roving tabindex: the list is a single Tab stop. Making every
                // row tabbable would put 50 rows and their buttons in the tab
                // order, so reaching the pager would take 150 presses.
                tabIndex={index === focusIndex ? 0 : -1}
                onFocus={() => setFocusIndex(index)}
                onKeyDown={(e) => move(e, index, server)}
              >
                <button
                  className={`icon-btn${isFavorite ? " is-favorite" : ""}`}
                  tabIndex={-1}
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleFavorite(server.id);
                  }}
                  title={isFavorite ? "Remove from favourites" : "Add to favourites"}
                >
                  <Star size={15} fill={isFavorite ? "currentColor" : "none"} />
                </button>

                <div style={{ minWidth: 0 }}>
                  <div className="server-name">
                    {server.password && <Lock size={12} color="var(--orange)" />}
                    <span
                      style={{
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {server.name || "(unnamed)"}
                    </span>
                    {server.firstPersonOnly && (
                      <span className="badge badge-red">1PP</span>
                    )}
                  </div>
                  <div className="server-addr mono">
                    {server.ip}:{server.gamePort}
                    {server.version && ` · ${server.version}`}
                  </div>
                </div>

                <div style={{ color: "var(--cyan)", fontSize: 12 }}>{server.map}</div>

                <div className="num" style={{ fontSize: 12.5, fontWeight: 600 }}>
                  <span
                    style={{
                      color:
                        server.players >= server.maxPlayers && server.maxPlayers > 0
                          ? "var(--orange)"
                          : "var(--text)",
                    }}
                  >
                    {server.players}
                  </span>
                  <span style={{ color: "var(--text-dim)" }}>/{server.maxPlayers}</span>
                </div>

                <div
                  className={`num mono ${pingClass(ping?.pingMs)}`}
                  style={{ fontSize: 12, fontWeight: 600 }}
                >
                  {ping ? (ping.online ? `${ping.pingMs} ms` : "—") : "…"}
                </div>

                <div>
                  {server.modCount > 0 ? (
                    <span className="badge badge-blue">{server.modCount} mods</span>
                  ) : (
                    <span className="badge badge-green">Vanilla</span>
                  )}
                </div>

                <button
                  className="btn btn-primary btn-sm"
                  tabIndex={-1}
                  onClick={(e) => {
                    e.stopPropagation();
                    onJoin(server);
                  }}
                >
                  <Play size={12} fill="currentColor" />
                  Join
                </button>
              </div>
            );
          })
        )}
      </div>

      <div className="pager">
        <span>
          {formatNumber(total)} server{total === 1 ? "" : "s"}
          {sort === "ping" && " · sorted by ping on this page"}
          {loading && items.length > 0 && " · refreshing…"}
        </span>

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button
            className="btn btn-sm"
            disabled={current <= 0}
            onClick={() => onPage(current - 1)}
          >
            <ChevronLeft size={14} />
          </button>
          <span>
            Page {current + 1} of {lastPage + 1}
          </span>
          <button
            className="btn btn-sm"
            disabled={current >= lastPage}
            onClick={() => onPage(current + 1)}
          >
            <ChevronRight size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}
