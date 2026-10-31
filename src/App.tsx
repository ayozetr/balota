// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  FolderCheck,
  Github,
  Heart,
  History,
  Loader2,
  RefreshCw,
  Server,
  Settings as SettingsIcon,
  Star,
  Terminal,
  X,
} from "lucide-react";

import logo from "./assets/logo.png";
import FilterBar from "./components/FilterBar";
import HistoryTab from "./components/HistoryTab";
import ModsTab from "./components/ModsTab";
import ServerModal from "./components/ServerModal";
import ServerTable from "./components/ServerTable";
import SettingsTab from "./components/SettingsTab";
import WindowControls from "./components/WindowControls";
import ResizeEdges from "./components/ResizeEdges";
import { useMaximized } from "./useMaximized";
import { useShortcuts } from "./useShortcuts";
import { useFocusTrap } from "./useFocusTrap";
import { useEscape } from "./useEscape";
import { useGamepad } from "./useGamepad";
import { useSpatialNav } from "./useSpatialNav";
import { formatNumber, timeAgo } from "./format";
import { playerNameError } from "./playerName";
import { KOFI_URL, REPO_URL } from "./links";
import * as api from "./api";
import type {
  AppConfig,
  ListStatus,
  MapCount,
  ModFilter,
  Perspective,
  PingResult,
  SavedFilters,
  ServerDetails,
  ServerPage,
  ServerRow,
  SortColumn,
  SortMode,
  SteamEnvironment,
} from "./types";

type Tab = "servers" | "favorites" | "history" | "mods" | "settings";

interface Toast {
  message: string;
  kind: "info" | "error";
}

const PAGE_SIZE = 50;
/** Columns that read high-to-low when you first pick them. */
const DESC_BY_DEFAULT: SortColumn[] = ["players"];

export default function App() {
  const [tab, setTab] = useState<Tab>("servers");
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [env, setEnv] = useState<SteamEnvironment | null>(null);
  const [status, setStatus] = useState<ListStatus | null>(null);
  const [page, setPage] = useState<ServerPage | null>(null);
  const [maps, setMaps] = useState<MapCount[]>([]);
  const [pings, setPings] = useState<Record<string, PingResult>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [loading, setLoading] = useState(true);
  const [listError, setListError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [directOpen, setDirectOpen] = useState(false);
  /** Ticks once a minute so "updated 3 minutes ago" stops lying. */
  const [, setClock] = useState(0);

  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [map, setMap] = useState("");
  const [mods, setMods] = useState<ModFilter>("all");
  const [perspective, setPerspective] = useState<Perspective>("all");
  const [hideEmpty, setHideEmpty] = useState(true);
  const [hideFull, setHideFull] = useState(false);
  const [hidePassword, setHidePassword] = useState(false);
  const [sort, setSort] = useState<SortMode>({ by: "players", desc: true });
  const [pageIndex, setPageIndex] = useState(0);

  const maximized = useMaximized();
  const pad = useGamepad();
  useSpatialNav();
  const toastTimer = useRef<number>();
  const searchRef = useRef<HTMLInputElement>(null);
  /** True once the saved filters have been applied, so the save effect below
   *  does not fire while they are still being restored. */
  const hydrated = useRef(false);
  /** Guards the list refresh. `refreshing` is state, so it updates a tick too
   *  late to stop a held-down F5. */
  const refreshingNow = useRef(false);

  const nameProblem = config ? playerNameError(config.playerName) : null;
  const joinBlockedReason = !env?.dayzFound
    ? "DayZ was not found on this system. Check the Settings tab."
    : nameProblem;

  const lastPage = page ? Math.max(0, Math.ceil(page.total / page.pageSize) - 1) : 0;
  useShortcuts({
    onSearch: () => searchRef.current?.focus(),
    onPrevPage: () => setPageIndex((i) => Math.max(0, i - 1)),
    onNextPage: () => setPageIndex((i) => Math.min(lastPage, i + 1)),
    onRefresh: () => void loadListRef.current?.(true),
  });

  // The frame's corners and shadow are CSS, so the maximised state has to
  // reach the stylesheet.
  useEffect(() => {
    document.documentElement.classList.toggle("maximized", maximized);
  }, [maximized]);

  useEffect(() => {
    const timer = window.setInterval(() => setClock((c) => c + 1), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const loadListRef = useRef<(force: boolean) => Promise<void>>();

  const notify = useCallback((message: string, kind: Toast["kind"] = "info") => {
    setToast({ message, kind });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 6000);
  }, []);

  useEffect(() => () => window.clearTimeout(toastTimer.current), []);

  // ---------------------------------------------------------------- boot --

  const loadList = useCallback(
    async (force: boolean) => {
      // Holding F5 used to start one 26 MB download per key repeat.
      if (refreshingNow.current) return;
      refreshingNow.current = true;
      setRefreshing(true);
      try {
        const fresh = await api.refreshServers(force);
        setStatus(fresh);
        setMaps(await api.serverMaps());
        setListError(null);
        if (fresh.warning) notify(fresh.warning, "error");
      } catch (e) {
        // Leaving `loading` true here is what painted "Loading servers…"
        // forever when the first fetch failed.
        setListError(String(e));
        setLoading(false);
        notify(String(e), "error");
      } finally {
        refreshingNow.current = false;
        setRefreshing(false);
      }
    },
    [notify],
  );
  loadListRef.current = loadList;

  useEffect(() => {
    (async () => {
      try {
        const [loadedConfig, loadedEnv] = await Promise.all([
          api.getConfig(),
          api.steamEnvironment(),
        ]);
        setConfig(loadedConfig);
        setEnv(loadedEnv);

        // Bring back the list the way it was left.
        const saved = loadedConfig.filters;
        if (saved) {
          setSearch(saved.search ?? "");
          setDebouncedSearch(saved.search ?? "");
          setMap(saved.map ?? "");
          setMods((saved.mods as ModFilter) ?? "all");
          setPerspective((saved.perspective as Perspective) ?? "all");
          setHideEmpty(saved.hideEmpty ?? true);
          setHideFull(saved.hideFull ?? false);
          setHidePassword(saved.hidePassword ?? false);
          setSort({
            by: (saved.sort as SortColumn) ?? "players",
            desc: saved.sortDesc ?? true,
          });
          if (saved.tab) setTab(saved.tab as Tab);
        }
      } catch (e) {
        notify(String(e), "error");
      }
      hydrated.current = true;
      await loadList(false);
    })();
  }, [loadList, notify]);

  // Debounce so typing does not fire a query per keystroke.
  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedSearch(search), 260);
    return () => window.clearTimeout(timer);
  }, [search]);

  // Any filter change sends you back to the first page.
  useEffect(() => {
    setPageIndex(0);
  }, [debouncedSearch, map, mods, perspective, hideEmpty, hideFull, hidePassword, sort, tab]);

  // Remember the filters. Debounced, because this writes a file.
  useEffect(() => {
    if (!hydrated.current) return;

    const filters: SavedFilters = {
      search: debouncedSearch,
      map: map || null,
      mods,
      perspective,
      hideEmpty,
      hideFull,
      hidePassword,
      sort: sort.by,
      sortDesc: sort.desc,
      tab,
    };

    const timer = window.setTimeout(() => {
      api.saveSettings({ filters }).catch(() => undefined);
    }, 600);
    return () => window.clearTimeout(timer);
  }, [debouncedSearch, map, mods, perspective, hideEmpty, hideFull, hidePassword, sort, tab]);

  const showingServers = tab === "servers" || tab === "favorites";

  // Depending on the whole config object would re-query the list on every
  // keystroke of the player name. The favourites are the only part the query
  // actually reads, so the effect keys off those.
  const favoritesKey = (config?.favorites ?? []).join(",");
  const onlyFavorites = tab === "favorites";

  useEffect(() => {
    if (!showingServers || !status) {
      // Without this the spinner outlived every early return.
      setLoading(false);
      return;
    }

    let cancelled = false;
    setLoading(true);

    api
      .queryServers({
        search: debouncedSearch,
        map: map || null,
        mods,
        perspective,
        hideEmpty,
        hideFull,
        hidePassword,
        onlyFavorites,
        // Only the favourites view reads this list, so a star clicked in the
        // main list no longer re-runs the query and re-pings 50 servers.
        favorites: onlyFavorites && favoritesKey ? favoritesKey.split(",") : [],
        // The backend has no pings to sort by; that ordering happens in the
        // table, over the rows it has actually probed.
        sort: sort.by === "ping" ? "players" : sort.by,
        sortDesc: sort.by === "ping" ? true : sort.desc,
        page: pageIndex,
        pageSize: PAGE_SIZE,
      })
      .then((result) => {
        if (cancelled) return;
        setPage(result);
        setListError(null);
      })
      .catch((e) => !cancelled && notify(String(e), "error"))
      .finally(() => !cancelled && setLoading(false));

    return () => {
      cancelled = true;
    };
  }, [
    showingServers,
    onlyFavorites,
    favoritesKey,
    status,
    debouncedSearch,
    map,
    mods,
    perspective,
    hideEmpty,
    hideFull,
    hidePassword,
    sort,
    pageIndex,
    notify,
  ]);

  // Ping only what is on screen: 50 UDP probes, not 21,000.
  useEffect(() => {
    const items = page?.items ?? [];
    if (items.length === 0) return;

    let cancelled = false;
    const ids = items.map((item) => `${item.ip}:${item.queryPort}`);
    const byId = new Map(items.map((item) => [`${item.ip}:${item.queryPort}`, item.id]));

    api
      .pingServers(ids)
      .then((results) => {
        if (cancelled) return;
        setPings((previous) => {
          const next = { ...previous };
          for (const result of results) {
            const rowId = byId.get(result.id);
            if (rowId) next[rowId] = result;
          }
          // The map grew for the whole session as pages were browsed; keep it
          // to what the current page can still show.
          const keep = new Set(items.map((item) => item.id));
          for (const key of Object.keys(next)) {
            if (!keep.has(key) && !previous[key]) delete next[key];
          }
          return next;
        });
      })
      .catch(() => undefined);

    return () => {
      cancelled = true;
    };
  }, [page]);

  // -------------------------------------------------------------- actions --

  async function toggleFavorite(id: string) {
    try {
      setConfig(await api.toggleFavorite(id));
    } catch (e) {
      notify(String(e), "error");
    }
  }

  /** Applies a settings change and persists just that field. */
  const updateSettings = useCallback(
    async (patch: Parameters<typeof api.saveSettings>[0]) => {
      try {
        setConfig(await api.saveSettings(patch));
        if (patch.customSteamRoot !== undefined) {
          setEnv(await api.steamEnvironment());
        }
      } catch (e) {
        notify(String(e), "error");
      }
    },
    [notify],
  );

  async function launchWith(details: ServerDetails) {
    if (joinBlockedReason) {
      notify(joinBlockedReason, "error");
      if (nameProblem) setTab("settings");
      return;
    }
    try {
      const outcome = await api.launchGame({
        connect: details.connect,
        modIds: details.mods.map((mod) => mod.id),
      });
      setConfig(
        await api.recordLaunch(
          details.server.id,
          details.server.name,
          details.server.map,
        ),
      );
      setSelected(null);
      notify(
        `Launching DayZ on ${details.server.name}` +
          (outcome.linkedMods.length
            ? ` with ${outcome.linkedMods.length} mod(s).`
            : "."),
      );
    } catch (e) {
      notify(String(e), "error");
    }
  }

  async function join(id: string) {
    if (joinBlockedReason) {
      notify(joinBlockedReason, "error");
      if (nameProblem) setTab("settings");
      return;
    }
    try {
      const details = await api.serverDetails(id, true);
      if (details.missingCount > 0) {
        setSelected(id);
        notify(
          `${details.missingCount} mod(s) missing — install them before joining.`,
          "error",
        );
        return;
      }
      await launchWith(details);
    } catch (e) {
      notify(String(e), "error");
    }
  }

  function sortBy(column: SortColumn) {
    setSort((previous) =>
      previous.by === column
        ? { by: column, desc: !previous.desc }
        : { by: column, desc: DESC_BY_DEFAULT.includes(column) },
    );
  }

  // ----------------------------------------------------------------- view --

  const favoriteCount = config?.favorites.length ?? 0;

  const navItems = useMemo(
    () =>
      [
        { id: "servers", label: "Servers", icon: Server },
        { id: "favorites", label: "Favourites", icon: Star, count: favoriteCount },
        { id: "history", label: "History", icon: History, count: config?.history.length },
        { id: "mods", label: "Mods", icon: FolderCheck },
        { id: "settings", label: "Settings", icon: SettingsIcon },
      ] as const,
    [favoriteCount, config?.history.length],
  );

  const openLink = (url: string) =>
    api.openUrl(url).catch(() => notify("Could not open the link. Is xdg-open installed?", "error"));

  return (
    <div className="app">
      <ResizeEdges />

      <aside className="sidebar">
        <div className="brand">
          <img className="brand-mark" src={logo} alt="" width={44} height={44} />
          <div>
            <div className="brand-name">Balota</div>
            <span className="brand-sub">DayZ · Linux</span>
          </div>
        </div>

        <nav className="nav">
          {navItems.map((item) => (
            <button
              key={item.id}
              className={`nav-item${tab === item.id ? " active" : ""}`}
              aria-current={tab === item.id ? "page" : undefined}
              onClick={() => setTab(item.id as Tab)}
            >
              <item.icon size={16} />
              {item.label}
              {"count" in item && item.count ? (
                <span className="nav-count">{item.count}</span>
              ) : null}
            </button>
          ))}
        </nav>

        <div className="sidebar-links">
          <button
            className="sidebar-link"
            onClick={() => openLink(REPO_URL)}
            title="View the source on GitHub"
          >
            <Github size={14} />
            ayozetr
          </button>
          <button
            className="sidebar-link"
            onClick={() => openLink(KOFI_URL)}
            title="Support Balota on Ko-fi"
            aria-label="Support Balota on Ko-fi"
          >
            <Heart size={14} />
            Ko-fi
          </button>
        </div>
        <div className="status-card">
          <div className="status-row">
            <span>Steam</span>
            <span style={{ color: !env ? "var(--text-muted)" : env.steamFound ? "var(--green)" : "var(--red)" }}>
              {!env ? "checking…" : env.steamFound ? (env.isFlatpak ? "Flatpak" : "Native") : "not found"}
            </span>
          </div>
          <div className="status-row">
            <span>DayZ</span>
            <span style={{ color: !env ? "var(--text-muted)" : env.dayzFound ? "var(--green)" : "var(--red)" }}>
              {!env ? "checking…" : env.dayzFound ? "ready" : "not found"}
            </span>
          </div>
          <div className="status-row">
            <span>Servers</span>
            <span>{status ? formatNumber(status.total) : "…"}</span>
          </div>
          {(pad.id || env?.steamDeck) && (
            <div className="status-row">
              <span>Gamepad</span>
              <span style={{ color: "var(--green)" }}>
                {env?.steamDeck ? "Deck" : "connected"}
              </span>
            </div>
          )}
        </div>

      </aside>

      <main className="main">
        {/* The header doubles as the title bar: dragging it moves the window.
            Interactive children keep their own clicks, since the drag only
            fires when the press lands on the region itself. */}
        <header className="header" data-tauri-drag-region>
          <PlayerNameField
            value={config?.playerName ?? ""}
            problem={nameProblem}
            disabled={!config}
            onCommit={(playerName) => updateSettings({ playerName })}
          />

          <div className="spacer" />

          {status && (
            <span style={{ fontSize: 11.5, color: "var(--text-muted)" }}>
              List updated {timeAgo(status.fetchedAt)}
            </span>
          )}

          <button
            className="btn btn-sm"
            onClick={() => loadList(true)}
            disabled={refreshing}
          >
            {refreshing ? (
              <Loader2 size={13} className="spin" />
            ) : (
              <RefreshCw size={13} />
            )}
            Refresh list
          </button>

          <button className="btn btn-sm" onClick={() => setDirectOpen(true)}>
            <Terminal size={13} />
            Direct connect
          </button>

          <WindowControls />
        </header>

        <div className="content">
          {env && !env.dayzFound && tab !== "settings" && (
            <div className="note">
              <AlertTriangle size={15} style={{ flexShrink: 0 }} />
              <span>
                DayZ was not found on this system, so launching is disabled. Check the
                Settings tab for details.
              </span>
            </div>
          )}

          {config && nameProblem && tab !== "settings" && (
            <div className="note">
              <AlertTriangle size={15} style={{ flexShrink: 0 }} />
              <span>
                {nameProblem} Servers commonly kick players who join with the default
                name, and the game gives no reason when they do.
              </span>
            </div>
          )}

          {showingServers && (
            <>
              <FilterBar
                search={search}
                onSearch={setSearch}
                searchRef={searchRef}
                maps={maps}
                map={map}
                onMap={setMap}
                mods={mods}
                onMods={setMods}
                perspective={perspective}
                onPerspective={setPerspective}
                hideEmpty={hideEmpty}
                onHideEmpty={setHideEmpty}
                hideFull={hideFull}
                onHideFull={setHideFull}
                hidePassword={hidePassword}
                onHidePassword={setHidePassword}
              />

              <ServerTable
                page={page}
                loading={loading}
                error={listError}
                onRetry={() => loadList(true)}
                favorites={config?.favorites ?? []}
                pings={pings}
                selectedId={selected}
                sort={sort}
                onSort={sortBy}
                onSelect={(server: ServerRow) => setSelected(server.id)}
                onToggleFavorite={toggleFavorite}
                onJoin={(server) => join(server.id)}
                pageIndex={pageIndex}
                onPage={setPageIndex}
                joinBlockedReason={joinBlockedReason}
                emptyMessage={
                  tab === "favorites"
                    ? "No favourites yet. Star a server to keep it here."
                    : "No servers match these filters."
                }
              />
            </>
          )}

          {tab === "history" && config && (
            <HistoryTab
              config={config}
              onOpen={(id) => setSelected(id)}
              onForget={async (id) => {
                try {
                  setConfig(await api.forgetLaunch(id));
                } catch (e) {
                  notify(String(e), "error");
                }
              }}
            />
          )}

          {tab === "mods" && (
            <ModsTab env={env} onError={(m) => notify(m, "error")} onNotice={notify} />
          )}

          {tab === "settings" && config && (
            <SettingsTab
              config={config}
              env={env}
              nameProblem={nameProblem}
              onChange={updateSettings}
              onRedetect={async () => {
                setEnv(await api.steamEnvironment(true));
                notify("Steam detection refreshed.");
              }}
              onNotice={notify}
              onError={(m) => notify(m, "error")}
              pad={pad}
            />
          )}
        </div>
      </main>

      {selected && (
        <ServerModal
          serverId={selected}
          onClose={() => setSelected(null)}
          onJoin={launchWith}
          onError={(m) => notify(m, "error")}
          onNotice={notify}
          joinBlockedReason={joinBlockedReason}
        />
      )}

      {directOpen && (
        <DirectConnect
          onClose={() => setDirectOpen(false)}
          onLookUp={(queryAddress) => {
            setDirectOpen(false);
            setSelected(queryAddress);
          }}
          onConnect={async (address) => {
            setDirectOpen(false);
            if (joinBlockedReason) {
              notify(joinBlockedReason, "error");
              return;
            }
            try {
              await api.launchGame({
                connect: address,
                modIds: [],
              });
              notify(`Launching DayZ on ${address} (no mods).`);
            } catch (e) {
              notify(String(e), "error");
            }
          }}
        />
      )}

      {toast && (
        <div
          className={`toast${toast.kind === "error" ? " error" : ""}`}
          role="status"
          aria-live="polite"
        >
          {toast.kind === "error" ? (
            <AlertTriangle size={15} color="var(--red)" />
          ) : (
            <CheckCircle2 size={15} color="var(--green)" />
          )}
          <span style={{ flex: 1 }}>{toast.message}</span>
          <button className="icon-btn" onClick={() => setToast(null)} aria-label="Dismiss">
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * The in-game name, kept locally while it is being typed.
 *
 * Every keystroke used to be a write to disk *and* a full config round trip,
 * which is also how a name typed here could overwrite a favourite starred a
 * moment earlier.
 */
function PlayerNameField({
  value,
  problem,
  disabled,
  onCommit,
}: {
  value: string;
  problem: string | null;
  disabled: boolean;
  onCommit: (name: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const dirty = useRef(false);

  // Follow the stored value unless the user is mid-edit.
  useEffect(() => {
    if (!dirty.current) setDraft(value);
  }, [value]);

  useEffect(() => {
    if (!dirty.current || draft === value) return;
    const timer = window.setTimeout(() => {
      dirty.current = false;
      onCommit(draft);
    }, 500);
    return () => window.clearTimeout(timer);
  }, [draft, value, onCommit]);

  return (
    <label className="field">
      In-game name
      <input
        type="text"
        value={draft}
        placeholder="Your name"
        disabled={disabled}
        aria-invalid={problem ? true : undefined}
        aria-label="In-game name"
        title={problem ?? "Passed to the game as -name="}
        className={problem ? "invalid" : undefined}
        style={{ width: 150 }}
        onChange={(e) => {
          dirty.current = true;
          setDraft(e.target.value);
        }}
        onBlur={() => {
          if (dirty.current) {
            dirty.current = false;
            onCommit(draft);
          }
        }}
      />
    </label>
  );
}

function DirectConnect({
  onClose,
  onConnect,
  onLookUp,
}: {
  onClose: () => void;
  onConnect: (address: string) => void;
  /** Opens the regular server panel, which resolves mods through the API. */
  onLookUp: (queryAddress: string) => void;
}) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  useEscape(onClose);
  const [address, setAddress] = useState("");
  const [queryPort, setQueryPort] = useState("27016");

  const trimmed = address.trim();
  // Each octet has to be a real octet and the port a real port: `999.999…`
  // used to sail through here and fail with a cryptic message from the
  // backend instead.
  const octet = "(25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)";
  const port = "(6553[0-5]|655[0-2]\\d|65[0-4]\\d\\d|6[0-4]\\d{3}|[1-5]?\\d{1,4})";
  const valid = new RegExp(`^${octet}(\\.${octet}){3}:${port}$`).test(trimmed);
  const ip = trimmed.split(":")[0];
  const validQueryPort = new RegExp(`^${port}$`).test(queryPort.trim());
  const canLookUp = valid && validQueryPort;

  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="modal"
        ref={trapRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="direct-connect-title"
        tabIndex={-1}
        style={{ width: 460 }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3 id="direct-connect-title" style={{ flex: 1, fontSize: 15 }}>
            Direct connect
          </h3>
          <button className="icon-btn" onClick={onClose} aria-label="Close">
            <X size={17} />
          </button>
        </div>

        <div className="modal-body">
          <label className="field" style={{ gap: 10, marginBottom: 10 }}>
            <span style={{ width: 78 }}>Address</span>
            <input
              data-autofocus
              type="text"
              className="mono"
              placeholder="192.223.24.12:2302"
              value={address}
              aria-invalid={trimmed !== "" && !valid ? true : undefined}
              style={{ flex: 1 }}
              onChange={(e) => setAddress(e.target.value)}
              onKeyDown={(e) =>
                e.key === "Enter" && canLookUp && onLookUp(`${ip}:${queryPort.trim()}`)
              }
            />
          </label>

          <label className="field" style={{ gap: 10 }}>
            <span style={{ width: 78 }}>Query port</span>
            <input
              type="text"
              className="mono"
              value={queryPort}
              aria-invalid={!validQueryPort ? true : undefined}
              style={{ width: 100 }}
              onChange={(e) => setQueryPort(e.target.value)}
            />
          </label>

          <p className="hint" style={{ marginTop: 12 }}>
            The address takes the <strong>game port</strong> (usually 2302). The query
            port is a different one — 27016 on most servers, sometimes game port + 1.
            Looking it up resolves the server's mods so they can be linked; connecting
            straight away skips that and joins with no mods.
          </p>
        </div>

        <div className="modal-foot">
          <div style={{ flex: 1 }} />
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn"
            disabled={!valid}
            onClick={() => onConnect(trimmed)}
            title="Join without resolving mods"
          >
            Join, no mods
          </button>
          <button
            className="btn btn-primary"
            disabled={!canLookUp}
            onClick={() => onLookUp(`${ip}:${queryPort.trim()}`)}
          >
            Look up mods
          </button>
        </div>
      </div>
    </div>
  );
}
