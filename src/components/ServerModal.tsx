// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import {
  CheckCircle2,
  Download,
  ExternalLink,
  Loader2,
  Play,
  RefreshCw,
  X,
} from "lucide-react";
import type { ServerDetails } from "../types";
import { modsInstalled, openWorkshopPage, serverDetails, subscribeMods } from "../api";

interface Props {
  serverId: string;
  onClose: () => void;
  onJoin: (details: ServerDetails) => void;
  onError: (message: string) => void;
  onNotice: (message: string) => void;
}

export default function ServerModal({
  serverId,
  onClose,
  onJoin,
  onError,
  onNotice,
}: Props) {
  const [details, setDetails] = useState<ServerDetails | null>(null);
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState(false);
  /** How many of the mods requested for download have landed so far. */
  const [progress, setProgress] = useState({ done: 0, total: 0 });

  async function load(refresh: boolean) {
    setLoading(true);
    try {
      setDetails(await serverDetails(serverId, refresh));
    } catch (e) {
      onError(String(e));
      onClose();
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverId]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function installMissing() {
    if (!details) return;
    const missing = details.mods.filter((m) => !m.installed).map((m) => m.id);
    if (missing.length === 0) return;

    setDownloading(true);
    setProgress({ done: 0, total: missing.length });

    try {
      const result = await subscribeMods(missing);
      if (result.warning) {
        onError(result.warning);
      } else {
        onNotice(
          `Subscribed to ${result.count} mod(s). Steam is downloading them in the background ` +
            `and will keep them updated from now on.`,
        );
      }
    } catch (e) {
      onError(String(e));
      setDownloading(false);
    }
  }

  // While a download runs, poll the folder instead of making the user press
  // Re-check. Steam gives no completion signal, so watching the disk is it.
  useEffect(() => {
    if (!downloading || !details) return;

    const pending = details.mods.filter((m) => !m.installed).map((m) => m.id);
    if (pending.length === 0) {
      setDownloading(false);
      return;
    }

    let cancelled = false;
    const timer = window.setInterval(async () => {
      try {
        const done = await modsInstalled(pending);
        if (cancelled) return;

        setProgress({ done: done.length, total: pending.length });
        if (done.length === pending.length) {
          setDownloading(false);
          await load(false);
          onNotice("All mods downloaded. You can join now.");
        }
      } catch {
        // A failed poll is not worth reporting; the next tick retries.
      }
    }, 2500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [downloading, details]);

  const server = details?.server;
  const missing = details?.missingCount ?? 0;
  const ready = details !== null && missing === 0;

  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div style={{ flex: 1, minWidth: 0 }}>
            <h3 style={{ fontSize: 15 }}>{server?.name ?? "Loading…"}</h3>
            <div className="mono" style={{ fontSize: 11.5, color: "var(--text-dim)" }}>
              {details?.connect}
              {server && ` · ${server.map} · ${server.players}/${server.maxPlayers}`}
              {server?.time && ` · ${server.time} in-game`}
            </div>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close">
            <X size={17} />
          </button>
        </div>

        <div className="modal-body">
          {details?.warning && <div className="note">{details.warning}</div>}

          {loading ? (
            <div className="empty">
              <Loader2 size={20} className="spin" />
              Querying the server…
            </div>
          ) : details && details.mods.length === 0 ? (
            <div className="note info">
              <CheckCircle2 size={16} />
              Vanilla server — no mods required.
            </div>
          ) : (
            <>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  marginBottom: 10,
                  fontSize: 12,
                  color: "var(--text-muted)",
                }}
              >
                <span>
                  {details?.mods.length} required mod
                  {details?.mods.length === 1 ? "" : "s"}
                  {missing > 0 && ` · ${missing} missing`}
                </span>
                <button
                  className="btn btn-ghost btn-sm"
                  style={{ marginLeft: "auto" }}
                  onClick={() => load(true)}
                >
                  <RefreshCw size={13} />
                  Re-check
                </button>
              </div>

              <div className="mod-list">
                {details?.mods.map((mod) => (
                  <div
                    key={mod.id}
                    className={`mod-item${mod.installed ? "" : " missing"}`}
                  >
                    {mod.installed ? (
                      <CheckCircle2 size={14} color="var(--green)" />
                    ) : (
                      <Download size={14} color="var(--orange)" />
                    )}
                    <span className="mod-name">{mod.name}</span>
                    <button
                      className="btn btn-ghost btn-sm"
                      onClick={() => openWorkshopPage(mod.id)}
                      title="Open the Workshop page"
                    >
                      <span className="mono">{mod.id}</span>
                      <ExternalLink size={12} />
                    </button>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <div className="modal-foot">
          {missing > 0 && (
            <button className="btn" onClick={installMissing} disabled={downloading}>
              {downloading ? (
                <Loader2 size={14} className="spin" />
              ) : (
                <Download size={14} />
              )}
              {downloading
                ? `Installing ${progress.done}/${progress.total}`
                : `Subscribe & install ${missing}`}
            </button>
          )}

          <div style={{ flex: 1 }} />

          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button
            className="btn btn-primary"
            disabled={!ready}
            title={
              ready ? "Launch DayZ and connect" : "Install the missing mods first"
            }
            onClick={() => details && onJoin(details)}
          >
            <Play size={13} fill="currentColor" />
            Join server
          </button>
        </div>
      </div>
    </div>
  );
}
