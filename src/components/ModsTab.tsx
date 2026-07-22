import { useEffect, useState } from "react";
import { ExternalLink, FolderOpen, Loader2, RefreshCw, Trash2 } from "lucide-react";
import type { InstalledMod, SteamEnvironment } from "../types";
import { installedMods, openWorkshopPage, pruneSymlinks } from "../api";
import { formatBytes } from "../format";

interface Props {
  env: SteamEnvironment | null;
  onError: (message: string) => void;
  onNotice: (message: string) => void;
}

export default function ModsTab({ env, onError, onNotice }: Props) {
  const [mods, setMods] = useState<InstalledMod[]>([]);
  const [loading, setLoading] = useState(true);
  const [withSizes, setWithSizes] = useState(false);

  async function load(sizes: boolean) {
    setLoading(true);
    try {
      setMods(await installedMods(sizes));
    } catch (e) {
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load(withSizes);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [withSizes]);

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

  return (
    <div className="panel" style={{ flex: 1 }}>
      <div className="panel-head">
        <h2 style={{ fontSize: 14 }}>Installed Workshop mods</h2>
        <span style={{ fontSize: 12, color: "var(--text-dim)" }}>
          {mods.length} mod{mods.length === 1 ? "" : "s"}
          {withSizes && totalSize > 0 && ` · ${formatBytes(totalSize)}`}
        </span>

        <div style={{ flex: 1 }} />

        <label className="toggle">
          <input
            type="checkbox"
            checked={withSizes}
            onChange={(e) => setWithSizes(e.target.checked)}
          />
          Show sizes
        </label>
        <button className="btn btn-sm" onClick={() => load(withSizes)}>
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
            No mods downloaded in {env.workshopDir}
          </div>
        ) : (
          <div className="card-grid">
            {mods.map((mod) => (
              <div key={mod.id} className="card">
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    marginBottom: 4,
                  }}
                >
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
                    onClick={() => openWorkshopPage(mod.id)}
                    title="Open the Workshop page"
                  >
                    <ExternalLink size={13} />
                  </button>
                </div>
                <div className="mono" style={{ fontSize: 11, color: "var(--text-dim)" }}>
                  {mod.id}
                  {withSizes && mod.sizeBytes > 0 && ` · ${formatBytes(mod.sizeBytes)}`}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
