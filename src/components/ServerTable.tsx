// SPDX-License-Identifier: GPL-3.0-or-later
import { memo, useEffect, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Loader2,
  Lock,
  Play,
  RefreshCw,
  ServerOff,
  Star,
  WifiOff,
} from "lucide-react";
import type { PingResult, ServerPage, ServerRow, SortColumn, SortMode } from "../types";
import { formatNumber, pingClass } from "../format";

interface Props {
  page: ServerPage | null;
  loading: boolean;
  /** Set when the list could not be loaded at all. */
  error: string | null;
  onRetry: () => void;
  favorites: string[];
  pings: Record<string, PingResult>;
  selectedId: string | null;
  sort: SortMode;
  onSort: (column: SortColumn) => void;
  onSelect: (server: ServerRow) => void;
  onToggleFavorite: (id: string) => void;
  onJoin: (server: ServerRow) => void;
  /** The page the parent believes it is on. */
  pageIndex: number;
  onPage: (page: number) => void;
  emptyMessage: string;
  /** Why joining is unavailable, if it is. */
  joinBlockedReason: string | null;
}

const COLUMNS: { key: SortColumn; label: string; numeric?: boolean }[] = [
  { key: "name", label: "Server" },
  { key: "map", label: "Map" },
  { key: "players", label: "Players", numeric: true },
  { key: "ping", label: "Ping", numeric: true },
];

/**
 * The player counts to actually show.
 *
 * Two sources disagree: the master list, which can be minutes or hours old,
 * and the A2S probe this table already fires at every visible row, which comes
 * straight from the server. The probe wins — it is the true number, and a
 * server browser that tells you a server is empty when thirty people are on it
 * is failing at its one job.
 *
 * The catch, and it is why the tooltip exists: "hide empty" and "hide full"
 * are applied by the backend over the *list* data, before paging. So a row can
 * legitimately show a count that its own filter would have excluded.
 */
function counts(server: ServerRow, ping: PingResult | undefined) {
  const live = !!ping?.online && ping.players != null;
  return {
    live,
    players: live ? ping!.players! : server.players,
    // A server reporting 0 slots is reporting nothing useful; keep the list's.
    maxPlayers: live && ping!.maxPlayers ? ping!.maxPlayers : server.maxPlayers,
  };
}

function ServerTable({
  page,
  loading,
  error,
  onRetry,
  favorites,
  pings,
  selectedId,
  sort,
  onSort,
  onSelect,
  onToggleFavorite,
  onJoin,
  pageIndex,
  onPage,
  emptyMessage,
  joinBlockedReason,
}: Props) {
  const rows = page?.items ?? [];

  // Ping is measured here rather than served by the API, so sorting by it can
  // only order what has already been probed: this page. Unmeasured and
  // unreachable servers sink to the bottom instead of pretending to be fast.
  let items = rows;
  if (sort.by === "ping") {
    items = [...rows].sort((a, b) => {
      const pa = pings[a.id];
      const pb = pings[b.id];
      const va = pa?.online ? (pa.pingMs ?? Infinity) : Infinity;
      const vb = pb?.online ? (pb.pingMs ?? Infinity) : Infinity;
      if (va === vb) return b.players - a.players;
      // Unreachable servers stay at the bottom in both directions.
      if (va === Infinity) return 1;
      if (vb === Infinity) return -1;
      return sort.desc ? vb - va : va - vb;
    });
  } else if (sort.by === "players") {
    // The backend ordered this page by the list's counts; the ones on screen
    // are the probed ones. Re-sorting the visible rows by what is actually
    // displayed keeps the column from looking shuffled. It only reorders
    // within the page — which page you are on is still the backend's call.
    items = [...rows].sort((a, b) => {
      const va = counts(a, pings[a.id]).players;
      const vb = counts(b, pings[b.id]).players;
      if (va !== vb) return sort.desc ? vb - va : va - vb;
      return a.name.localeCompare(b.name);
    });
  }

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
    // Only the row itself; a key pressed on a button inside it is that
    // button's business.
    if (e.target !== e.currentTarget) return;

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
        if (!joinBlockedReason) onJoin(server);
        break;
    }
  }

  const liveCount = items.reduce(
    (n, server) => n + (counts(server, pings[server.id]).live ? 1 : 0),
    0,
  );

  const total = page?.total ?? 0;
  const pageSize = page?.pageSize ?? 50;
  // The parent's page, not the one the last reply happened to carry: clicking
  // ► twice while a query was in flight used to send the same page number
  // again and lose the second click.
  const lastPage = Math.max(0, Math.ceil(total / pageSize) - 1);
  const current = Math.min(pageIndex, lastPage);

  return (
    <div className="panel table">
      <div className="row row-head">
        <span />
        {COLUMNS.map((column) => {
          const active = sort.by === column.key;
          return (
            <button
              key={column.key}
              className={column.numeric ? "num" : undefined}
              onClick={() => onSort(column.key)}
              aria-sort={active ? (sort.desc ? "descending" : "ascending") : "none"}
              title={`Sort by ${column.label.toLowerCase()}${
                active ? (sort.desc ? " (ascending)" : " (descending)") : ""
              }`}
            >
              {column.label} {active && (sort.desc ? "▾" : "▴")}
            </button>
          );
        })}
        <span>Mods</span>
        <span />
      </div>

      <div className="rows">
        {error ? (
          <div className="empty">
            <WifiOff size={22} />
            <span style={{ maxWidth: 460, textAlign: "center" }}>{error}</span>
            <button className="btn btn-sm" onClick={onRetry}>
              <RefreshCw size={13} />
              Try again
            </button>
          </div>
        ) : loading && items.length === 0 ? (
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
            const { players, maxPlayers, live } = counts(server, ping);

            return (
              <div
                key={server.id}
                className={`row row-item${selectedId === server.id ? " selected" : ""}`}
                onClick={() => onSelect(server)}
                role="button"
                aria-label={`${server.name}, ${players} of ${maxPlayers} players`}
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
                  aria-label={
                    isFavorite
                      ? `Remove ${server.name} from favourites`
                      : `Add ${server.name} to favourites`
                  }
                >
                  <Star size={15} fill={isFavorite ? "currentColor" : "none"} />
                </button>

                <div style={{ minWidth: 0 }}>
                  <div className="server-name">
                    {server.password && <Lock size={12} color="var(--orange)" />}
                    <span className="clamp">{server.name || "(unnamed)"}</span>
                    {server.firstPersonOnly && (
                      <span className="badge badge-red">1PP</span>
                    )}
                  </div>
                  <div className="server-addr mono clamp">
                    {server.ip}:{server.gamePort}
                    {server.version && ` · ${server.version}`}
                  </div>
                </div>

                <div style={{ color: "var(--cyan)", fontSize: 12 }} className="clamp">
                  {server.map}
                </div>

                <div
                  className="num"
                  style={{ fontSize: 12.5, fontWeight: 600 }}
                  title={
                    live
                      ? `${players}/${maxPlayers} right now, measured directly from the server` +
                        (players !== server.players
                          ? ` (the list still says ${server.players}). The empty/full filters use the list, so they can lag behind.`
                          : "")
                      : "From the server list; not measured yet"
                  }
                >
                  <span
                    style={{
                      color:
                        players >= maxPlayers && maxPlayers > 0
                          ? "var(--orange)"
                          : "var(--text)",
                    }}
                  >
                    {players}
                  </span>
                  <span style={{ color: "var(--text-muted)" }}>/{maxPlayers}</span>
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
                  disabled={!!joinBlockedReason}
                  title={joinBlockedReason ?? "Launch DayZ and connect"}
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
          {sort.by === "ping" && " · sorted by ping on this page"}
          {sort.by === "players" && liveCount > 0 && " · player counts measured live"}
          {loading && items.length > 0 && " · refreshing…"}
        </span>

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <button
            className="btn btn-sm"
            disabled={current <= 0}
            onClick={() => onPage(current - 1)}
            aria-label="Previous page"
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
            aria-label="Next page"
          >
            <ChevronRight size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}

/* The pad state lives in App and updates as fast as the controller reports, so
   without this the whole table re-rendered on every stick wobble. */
export default memo(ServerTable);
