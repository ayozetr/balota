// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import {
  AlertTriangle,
  CheckSquare,
  ExternalLink,
  FolderOpen,
  Loader2,
  RefreshCw,
  Square,
  Trash2,
  XCircle,
} from "lucide-react";
import type { InstalledMod, SteamEnvironment } from "../types";
import {
  deleteMods,
  installedMods,
  modStates,
  openWorkshopPage,
  pruneSymlinks,
  unsubscribeMods,
} from "../api";
import { formatBytes } from "../format";

interface Props {
  env: SteamEnvironment | null;
  onError: (message: string) => void;
  onNotice: (message: string) => void;
}

export default function ModsTab({ env, onError, onNotice }: Props) {
  const [mods, setMods] = useState<InstalledMod[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  /** IDs Steam does not track — leftovers from a plain download. */
  const [orphans, setOrphans] = useState<Set<number>>(new Set());

  async function load() {
    setLoading(true);
    try {
      const list = await installedMods(true);
      setMods(list);
      setSelected(new Set());

      // Which ones Steam actually knows about. If the query fails (Steam
      // closed) the list still renders, just without the warning badges.
      try {
        const states = await modStates(list.map((mod) => mod.id));
        setOrphans(new Set(states.filter((s) => !s.subscribed).map((s) => s.id)));
      } catch {
        setOrphans(new Set());
      }
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function toggle(id: number) {
    setSelected((previous) => {
      const next = new Set(previous);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }

  const allSelected = mods.length > 0 && selected.size === mods.length;

  /** Unsubscribing is how a subscribed mod leaves: Steam deletes the files and
   *  stops tracking it. Leftover downloads have no subscription to drop, so
   *  those are removed from disk instead. */
  async function removeSelected(ids: number[]) {
    if (ids.length === 0) return;

    const toUnsubscribe = ids.filter((id) => !orphans.has(id));
    const toDelete = ids.filter((id) => orphans.has(id));

    setBusy(true);
    try {
      const parts: string[] = [];
      if (toUnsubscribe.length > 0) {
        const count = await unsubscribeMods(toUnsubscribe);
        parts.push(`unsubscribed from ${count}`);
      }
      if (toDelete.length > 0) {
        const count = await deleteMods(toDelete);
        parts.push(`deleted ${count} untracked`);
      }
      onNotice(`Done: ${parts.join(", ")}. Steam may take a moment to tidy up.`);
      // Give Steam time to remove the files before re-reading the folder.
      window.setTimeout(() => void load(), 2500);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function cleanup() {
    try {
      const removed = await pruneSymlinks();
      onNotice(
        removed === 0
          ? "No dangling mod links found."
          : `Removed ${removed} dangling mod link(s).`,
      );
    } catch (e) {
      onError(String(e));
    }
  }

  const totalSize = mods.reduce((sum, mod) => sum + mod.sizeBytes, 0);
  const selectedSize = mods
    .filter((mod) => selected.has(mod.id))
    .reduce((sum, mod) => sum + mod.sizeBytes, 0);

  return (
    <div className="panel" style={{ flex: 1 }}>
      <div className="panel-head">
        <h2 style={{ fontSize: 14 }}>Installed Workshop mods</h2>
        <span style={{ fontSize: 12, color: "var(--text-dim)" }}>
          {mods.length} mod{mods.length === 1 ? "" : "s"}
          {totalSize > 0 && ` · ${formatBytes(totalSize)}`}
          {selected.size > 0 && ` · ${selected.size} selected (${formatBytes(selectedSize)})`}
        </span>

        <div style={{ flex: 1 }} />

        {mods.length > 0 && (
          <button
            className="btn btn-sm"
            onClick={() =>
              setSelected(allSelected ? new Set() : new Set(mods.map((m) => m.id)))
            }
          >
            {allSelected ? <Square size={13} /> : <CheckSquare size={13} />}
            {allSelected ? "Deselect all" : "Select all"}
          </button>
        )}

        {selected.size > 0 && (
          <button
            className="btn btn-sm"
            onClick={() => removeSelected([...selected])}
            disabled={busy}
            title="Unsubscribe in Steam; untracked leftovers are deleted from disk"
          >
            {busy ? <Loader2 size={13} className="spin" /> : <XCircle size={13} />}
            Remove {selected.size}
          </button>
        )}

        <button className="btn btn-sm" onClick={load} disabled={loading}>
          <RefreshCw size={13} />
          Reload
        </button>
        <button
          className="btn btn-sm"
          onClick={cleanup}
          title="Delete @links inside the DayZ folder whose mod is gone"
        >
          <Trash2 size={13} />
          Clean up links
        </button>
      </div>

      <div className="panel-pad" style={{ flex: 1 }}>
        {loading ? (
          <div className="empty">
            <Loader2 size={20} className="spin" />
            Reading the Workshop folder…
          </div>
        ) : !env?.workshopDir ? (
          <div className="empty">
            <FolderOpen size={22} />
            No Workshop folder yet. It appears once you subscribe to your first DayZ mod.
          </div>
        ) : mods.length === 0 ? (
          <div className="empty">
            <FolderOpen size={22} />
            No mods installed.
          </div>
        ) : (
          <>
            {orphans.size > 0 && (
              <div className="note" style={{ marginBottom: 12 }}>
                <AlertTriangle size={15} style={{ flexShrink: 0, marginTop: 1 }} />
                <span>
                  {orphans.size} mod(s) are on disk but not subscribed, so Steam neither
                  updates nor removes them. Select them and press Remove to delete them
                  for good.
                </span>
              </div>
            )}

            <div className="card-grid">
              {mods.map((mod) => {
                const isOrphan = orphans.has(mod.id);
                return (
                  <div
                    key={mod.id}
                    className="card"
                    onClick={() => toggle(mod.id)}
                    style={{
                      cursor: "pointer",
                      ...(selected.has(mod.id)
                        ? { borderColor: "var(--red)", background: "rgba(255,59,48,0.07)" }
                        : {}),
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        marginBottom: 4,
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={selected.has(mod.id)}
                        onChange={() => toggle(mod.id)}
                        onClick={(e) => e.stopPropagation()}
                      />
                      <span
                        style={{
                          flex: 1,
                          fontWeight: 550,
                          fontSize: 13,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {mod.name}
                      </span>
                      <button
                        className="icon-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          openWorkshopPage(mod.id);
                        }}
                        title="Open the Workshop page"
                      >
                        <ExternalLink size={13} />
                      </button>
                    </div>

                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                        fontSize: 11,
                        color: "var(--text-dim)",
                      }}
                    >
                      <span className="mono">{mod.id}</span>
                      {mod.sizeBytes > 0 && <span>· {formatBytes(mod.sizeBytes)}</span>}
                      {isOrphan && (
                        <span className="badge badge-gray" style={{ marginLeft: "auto" }}>
                          not subscribed
                        </span>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
