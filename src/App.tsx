import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  FolderCheck,
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
import { formatNumber, timeAgo } from "./format";
import * as api from "./api";
import type {
  AppConfig,
  ListStatus,
  MapCount,
  ModFilter,
  Perspective,
  PingResult,
  ServerDetails,
  ServerPage,
  ServerRow,
  SortBy,
  SteamEnvironment,
} from "./types";

type Tab = "servers" | "favorites" | "history" | "mods" | "settings";

interface Toast {
  message: string;
  kind: "info" | "error";
}

const PAGE_SIZE = 50;

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
  const [refreshing, setRefreshing] = useState(false);
  const [directOpen, setDirectOpen] = useState(false);

  const [search, setSearch] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");
  const [map, setMap] = useState("");
  const [mods, setMods] = useState<ModFilter>("all");
  const [perspective, setPerspective] = useState<Perspective>("all");
  const [hideEmpty, setHideEmpty] = useState(true);
  const [hideFull, setHideFull] = useState(false);
  const [hidePassword, setHidePassword] = useState(false);
  const [sort, setSort] = useState<SortBy>("players");
  const [pageIndex, setPageIndex] = useState(0);

  const toastTimer = useRef<number>();

  const notify = useCallback((message: string, kind: Toast["kind"] = "info") => {
    setToast({ message, kind });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 6000);
  }, []);

  // ---------------------------------------------------------------- boot --

  const loadList = useCallback(
    async (force: boolean) => {
      setRefreshing(true);
      try {
        setStatus(await api.refreshServers(force));
        setMaps(await api.serverMaps());
      } catch (e) {
        notify(String(e), "error");
      } finally {
        setRefreshing(false);
      }
    },
    [notify],
  );

  useEffect(() => {
    (async () => {
      try {
        const [loadedConfig, loadedEnv] = await Promise.all([
          api.getConfig(),
          api.steamEnvironment(),
        ]);
        setConfig(loadedConfig);
        setEnv(loadedEnv);
      } catch (e) {
        notify(String(e), "error");
      }
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

  const showingServers = tab === "servers" || tab === "favorites";

  // Depending on the whole config object would re-query the list on every
  // keystroke of the player name. The favourites are the only part the query
  // actually reads, so the effect keys off those.
  const favoritesKey = (config?.favorites ?? []).join(",");

  useEffect(() => {
    if (!showingServers || !status) return;

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
        onlyFavorites: tab === "favorites",
        favorites: favoritesKey ? favoritesKey.split(",") : [],
        sort,
        page: pageIndex,
        pageSize: PAGE_SIZE,
      })
      .then((result) => !cancelled && setPage(result))
      .catch((e) => !cancelled && notify(String(e), "error"))
      .finally(() => !cancelled && setLoading(false));

    return () => {
      cancelled = true;
    };
  }, [
    showingServers,
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
    tab,
    notify,
  ]);

  // Ping only what is on screen: 50 UDP probes, not 21,000.
  useEffect(() => {
    const items = page?.items ?? [];
    if (items.length === 0) return;

    let cancelled = false;
    const ids = items.map((item) => `${item.ip}:${item.queryPort}`);

    api
      .pingServers(ids)
      .then((results) => {
        if (cancelled) return;
        setPings((previous) => {
          const next = { ...previous };
          for (const result of results) {
            const row = items.find(
              (item) => `${item.ip}:${item.queryPort}` === result.id,
            );
            if (row) next[row.id] = result;
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

  async function updateConfig(next: AppConfig) {
    setConfig(next);
    try {
      await api.saveConfig(next);
      if (next.customSteamRoot !== config?.customSteamRoot) {
        setEnv(await api.steamEnvironment());
      }
    } catch (e) {
      notify(String(e), "error");
    }
  }

  async function launchWith(details: ServerDetails) {
    try {
      const outcome = await api.launchGame({
        connect: details.connect,
        modIds: details.mods.map((mod) => mod.id),
        playerName: config?.playerName,
        extraArgs: config?.extraLaunchArgs,
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

  return (
    <div className="app">
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

        <div className="status-card">
          <div className="status-row">
            <span>Steam</span>
            <span style={{ color: env?.steamFound ? "var(--green)" : "var(--red)" }}>
              {env?.steamFound ? (env.isFlatpak ? "Flatpak" : "Native") : "not found"}
            </span>
          </div>
          <div className="status-row">
            <span>DayZ</span>
            <span style={{ color: env?.dayzFound ? "var(--green)" : "var(--red)" }}>
              {env?.dayzFound ? "ready" : "not found"}
            </span>
          </div>
          <div className="status-row">
            <span>Servers</span>
            <span>{status ? formatNumber(status.total) : "…"}</span>
          </div>
        </div>
      </aside>

      <main className="main">
        <header className="header">
          <label className="field">
            In-game name
            <input
              type="text"
              value={config?.playerName ?? ""}
              placeholder="Survivor"
              style={{ width: 150 }}
              onChange={(e) =>
                config && updateConfig({ ...config, playerName: e.target.value })
              }
            />
          </label>

          <div className="spacer" />

          {status && (
            <span style={{ fontSize: 11.5, color: "var(--text-dim)" }}>
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

          {showingServers && (
            <>
              <FilterBar
                search={search}
                onSearch={setSearch}
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
                favorites={config?.favorites ?? []}
                pings={pings}
                selectedId={selected}
                sort={sort}
                onSort={setSort}
                onSelect={(server: ServerRow) => setSelected(server.id)}
                onToggleFavorite={toggleFavorite}
                onJoin={(server) => join(server.id)}
                onPage={setPageIndex}
                emptyMessage={
                  tab === "favorites"
                    ? "No favourites yet. Star a server to keep it here."
                    : "No servers match these filters."
                }
              />
            </>
          )}

          {tab === "history" && config && (
            <HistoryTab config={config} onOpen={(id) => setSelected(id)} />
          )}

          {tab === "mods" && (
            <ModsTab env={env} onError={(m) => notify(m, "error")} onNotice={notify} />
          )}

          {tab === "settings" && config && (
            <SettingsTab
              config={config}
              env={env}
              onChange={updateConfig}
              onRedetect={async () => {
                setEnv(await api.steamEnvironment(true));
                notify("Steam detection refreshed.");
              }}
              onNotice={notify}
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
            try {
              await api.launchGame({
                connect: address,
                modIds: [],
                playerName: config?.playerName,
                extraArgs: config?.extraLaunchArgs,
              });
              notify(`Launching DayZ on ${address} (no mods).`);
            } catch (e) {
              notify(String(e), "error");
            }
          }}
        />
      )}

      {toast && (
        <div className={`toast${toast.kind === "error" ? " error" : ""}`}>
          {toast.kind === "error" ? (
            <AlertTriangle size={15} color="var(--red)" />
          ) : (
            <CheckCircle2 size={15} color="var(--green)" />
          )}
          <span style={{ flex: 1 }}>{toast.message}</span>
          <button className="icon-btn" onClick={() => setToast(null)}>
            <X size={14} />
          </button>
        </div>
      )}
    </div>
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
  const [address, setAddress] = useState("");
  const [queryPort, setQueryPort] = useState("27016");

  const trimmed = address.trim();
  const valid = /^\d{1,3}(\.\d{1,3}){3}:\d{1,5}$/.test(trimmed);
  const ip = trimmed.split(":")[0];
  const canLookUp = valid && /^\d{1,5}$/.test(queryPort.trim());

  return (
    <div className="overlay" onClick={onClose}>
      <div
        className="modal"
        style={{ width: 460 }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3 style={{ flex: 1, fontSize: 15 }}>Direct connect</h3>
          <button className="icon-btn" onClick={onClose}>
            <X size={17} />
          </button>
        </div>

        <div className="modal-body">
          <label className="field" style={{ gap: 10, marginBottom: 10 }}>
            <span style={{ width: 78 }}>Address</span>
            <input
              autoFocus
              type="text"
              className="mono"
              placeholder="192.223.24.12:2302"
              value={address}
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
