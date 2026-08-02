// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useRef, useState } from "react";
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
import ConfirmDialog from "./ConfirmDialog";

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
  const [confirming, setConfirming] = useState(false);

  const alive = useRef(true);
  const reloadTimer = useRef<number>();

  async function load() {
    setLoading(true);
    try {
      const list = await installedMods(true);
      if (!alive.current) return;
      setMods(list);
      setSelected(new Set());

      // Which ones Steam actually knows about. If the query fails (Steam
      // closed) the list still renders, just without the warning badges.
      try {
        const states = await modStates(list.map((mod) => mod.id));
        if (alive.current) {
          setOrphans(new Set(states.filter((s) => !s.subscribed).map((s) => s.id)));
        }
      } catch {
        if (alive.current) setOrphans(new Set());
      }
    } catch (e) {
      if (alive.current) onError(String(e));
    } finally {
      if (alive.current) setLoading(false);
    }
  }

  useEffect(() => {
    alive.current = true;
    void load();
    return () => {
      alive.current = false;
      window.clearTimeout(reloadTimer.current);
    };
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

  const chosen = mods.filter((mod) => selected.has(mod.id));
  const toDelete = chosen.filter((mod) => orphans.has(mod.id));
  const toUnsubscribe = chosen.filter((mod) => !orphans.has(mod.id));

  /** Unsubscribing is how a subscribed mod leaves: Steam deletes the files and
   *  stops tracking it. Leftover downloads have no subscription to drop, so
   *  those are removed from disk instead. */
  async function removeSelected() {
    setConfirming(false);
    if (chosen.length === 0) return;

    setBusy(true);
    try {
      const parts: string[] = [];
      if (toUnsubscribe.length > 0) {
        const count = await unsubscribeMods(toUnsubscribe.map((m) => m.id));
        parts.push(`unsubscribed from ${count}`);
      }
      if (toDelete.length > 0) {
        const count = await deleteMods(toDelete.map((m) => m.id));
        parts.push(`deleted ${count} untracked`);
      }
      onNotice(`Done: ${parts.join(", ")}. Steam may take a moment to tidy up.`);
      // Give Steam time to remove the files before re-reading the folder.
      reloadTimer.current = window.setTimeout(() => void load(), 2500);
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
        removed.length === 0
          ? "No dangling mod links found."
          : `Removed ${removed.length} dangling mod link(s): ${removed.join(", ")}.`,
      );
    } catch (e) {
      onError(String(e));
    }
  }

  const totalSize = mods.reduce((sum, mod) => sum + mod.sizeBytes, 0);
  const selectedSize = chosen.reduce((sum, mod) => sum + mod.sizeBytes, 0);

  return (
    <div className="panel" style={{ flex: 1 }}>
      <div className="panel-head">
        <h2 style={{ fontSize: 14 }}>Installed Workshop mods</h2>
        <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
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
            onClick={() => setConfirming(true)}
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
        ) : !env?.workshopDirs.length ? (
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
                const isSelected = selected.has(mod.id);
                return (
                  <div
                    key={mod.id}
                    className={`card${isSelected ? " selected" : ""}`}
                    role="checkbox"
                    aria-checked={isSelected}
                    aria-label={`${mod.name}, ${formatBytes(mod.sizeBytes)}`}
                    tabIndex={0}
                    onClick={() => toggle(mod.id)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        toggle(mod.id);
                      }
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
                        checked={isSelected}
                        tabIndex={-1}
                        readOnly
                        aria-hidden="true"
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
                        tabIndex={-1}
                        onClick={(e) => {
                          e.stopPropagation();
                          openWorkshopPage(mod.id).catch(() =>
                            onError("Could not open the Workshop page."),
                          );
                        }}
                        title="Open the Workshop page"
                        aria-label={`Open the Workshop page for ${mod.name}`}
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
                        color: "var(--text-muted)",
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

      {confirming && (
        <ConfirmDialog
          destructive={toDelete.length > 0}
          title={`Remove ${chosen.length} mod${chosen.length === 1 ? "" : "s"}?`}
          confirmLabel={`Remove ${chosen.length}`}
          onCancel={() => setConfirming(false)}
          onConfirm={removeSelected}
          body={
            <>
              <p style={{ marginBottom: 10 }}>
                This frees <strong>{formatBytes(selectedSize)}</strong>.
              </p>
              <ul style={{ margin: "0 0 10px 18px", fontSize: 12.5, lineHeight: 1.7 }}>
                {toUnsubscribe.length > 0 && (
                  <li>
                    <strong>{toUnsubscribe.length}</strong> will be unsubscribed in Steam.
                    Steam deletes the files; subscribing again re-downloads them.
                  </li>
                )}
                {toDelete.length > 0 && (
                  <li>
                    <strong>{toDelete.length}</strong> are not tracked by Steam and will be{" "}
                    <strong>deleted from disk. This cannot be undone</strong>, and getting
                    them back means downloading them again.
                  </li>
                )}
              </ul>
              <p className="hint" style={{ margin: 0 }}>
                {chosen
                  .slice(0, 6)
                  .map((mod) => mod.name)
                  .join(", ")}
                {chosen.length > 6 && ` and ${chosen.length - 6} more`}
              </p>
            </>
          }
        />
      )}
    </div>
  );
}
