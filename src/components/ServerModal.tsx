// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useRef, useState } from "react";
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
import { useFocusTrap } from "../useFocusTrap";
import { useEscape } from "../useEscape";
import { formatBytes } from "../format";
import {
  modSizes,
  modsInstalled,
  openWorkshopPage,
  serverDetails,
  subscribeMods,
} from "../api";

interface Props {
  serverId: string;
  onClose: () => void;
  onJoin: (details: ServerDetails) => void;
  onError: (message: string) => void;
  onNotice: (message: string) => void;
  /** Blocks joining when the game or the in-game name is not ready. */
  joinBlockedReason?: string | null;
}

/** No progress for this long means Steam is not downloading. */
const STALL_MS = 90_000;
const POLL_MS = 2_500;

export default function ServerModal({
  serverId,
  onClose,
  onJoin,
  onError,
  onNotice,
  joinBlockedReason,
}: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  const [details, setDetails] = useState<ServerDetails | null>(null);
  const [loading, setLoading] = useState(true);
  /** The IDs Steam accepted, or null when no download is running. */
  const [awaiting, setAwaiting] = useState<number[] | null>(null);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [stalled, setStalled] = useState(false);
  /** Total download size of the missing mods, once Steam has been asked. */
  const [download, setDownload] = useState<{ bytes: number; unknown: number[] } | null>(
    null,
  );
  const alive = useRef(true);

  useEscape(onClose);

  async function load(refresh: boolean) {
    setLoading(true);
    try {
      const fresh = await serverDetails(serverId, refresh);
      if (alive.current) setDetails(fresh);
    } catch (e) {
      if (alive.current) {
        onError(String(e));
        onClose();
      }
    } finally {
      if (alive.current) setLoading(false);
    }
  }

  useEffect(() => {
    alive.current = true;
    void load(true);
    return () => {
      alive.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverId]);

  // Ask Steam how big this is going to be. A 40-mod server can be several
  // gigabytes, and until now the only way to find out was to start it and
  // watch. Failing is fine: the button just goes back to not saying a size.
  useEffect(() => {
    const missing = (details?.mods ?? []).filter((m) => !m.installed).map((m) => m.id);
    if (missing.length === 0) {
      setDownload(null);
      return;
    }

    let cancelled = false;
    modSizes(missing)
      .then((reply) => {
        if (cancelled) return;
        setDownload({
          bytes: reply.items.reduce((sum, item) => sum + item.sizeBytes, 0),
          unknown: reply.unknown,
        });
      })
      .catch(() => !cancelled && setDownload(null));

    return () => {
      cancelled = true;
    };
  }, [details]);

  async function installMissing() {
    if (!details) return;
    const missing = details.mods.filter((m) => !m.installed).map((m) => m.id);
    if (missing.length === 0) return;

    setStalled(false);
    setProgress({ done: 0, total: missing.length });

    try {
      const result = await subscribeMods(missing);
      if (!alive.current) return;

      if (result.warning) onError(result.warning);

      // Only wait for what Steam actually took. Waiting on the whole list was
      // how "Installing 8/10" got stuck forever when two mods had been pulled
      // from the Workshop.
      const accepted = result.accepted ?? [];
      if (accepted.length === 0) {
        setAwaiting(null);
        if (!result.warning) {
          onError("Steam did not accept any of these mods. Try again in a moment.");
        }
        return;
      }

      setAwaiting(accepted);
      setProgress({ done: 0, total: accepted.length });
      if (!result.warning) {
        onNotice(
          `Subscribed to ${result.count} mod(s). Steam is downloading them in the background ` +
            `and will keep them updated from now on.`,
        );
      }
    } catch (e) {
      if (alive.current) {
        onError(String(e));
        setAwaiting(null);
      }
    }
  }

  // While a download runs, poll the folder instead of making the user press
  // Re-check. Steam gives no completion signal, so watching the disk is it.
  useEffect(() => {
    if (!awaiting || awaiting.length === 0) return;

    let cancelled = false;
    let lastProgressAt = performance.now();
    let seen = 0;

    const timer = window.setInterval(async () => {
      try {
        const done = await modsInstalled(awaiting);
        if (cancelled) return;

        if (done.length !== seen) {
          seen = done.length;
          lastProgressAt = performance.now();
          setStalled(false);
        }
        setProgress({ done: done.length, total: awaiting.length });

        if (done.length === awaiting.length) {
          setAwaiting(null);
          await load(false);
          onNotice("All mods downloaded. You can join now.");
          return;
        }

        // Steam not moving for a minute and a half is not "still working".
        if (performance.now() - lastProgressAt > STALL_MS) {
          setStalled(true);
        }
      } catch {
        // A failed poll is not worth reporting; the next tick retries.
      }
    }, POLL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [awaiting]);

  const server = details?.server;
  const missing = details?.missingCount ?? 0;
  const downloading = awaiting !== null;
  const ready = details !== null && missing === 0 && !joinBlockedReason;

  const joinTitle = joinBlockedReason
    ? joinBlockedReason
    : missing > 0
      ? "Install the missing mods first"
      : "Launch DayZ and connect";

  return (
    <div
      className="overlay"
      // Only a press that both starts and ends on the backdrop closes it.
      // Dragging out of the dialog used to dismiss it mid-download.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="modal"
        ref={trapRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="server-modal-title"
        tabIndex={-1}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div style={{ flex: 1, minWidth: 0 }}>
            <h3 id="server-modal-title" className="clamp" style={{ fontSize: 15 }}>
              {server?.name ?? "Loading…"}
            </h3>
            <div className="mono clamp" style={{ fontSize: 11.5, color: "var(--text-muted)" }}>
              {details?.connect}
              {server && ` · ${server.map} · ${server.players}/${server.maxPlayers}`}
              {server?.time && ` · ${server.time} in-game`}
            </div>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close" aria-label="Close">
            <X size={17} />
          </button>
        </div>

        <div className="modal-body">
          {details?.warning && <div className="note">{details.warning}</div>}

          {stalled && (
            <div className="note">
              Steam has not downloaded anything for a while. Check its Downloads page — it
              may be paused, or queued behind another game.
            </div>
          )}

          {/* These are the ones that hang a download forever, so say so before
              the user starts it rather than after. */}
          {!downloading && download && download.unknown.length > 0 && (
            <div className="note">
              Steam does not recognise {download.unknown.length} of the required mod
              {download.unknown.length === 1 ? "" : "s"} ({download.unknown.join(", ")}).
              They have probably been removed from the Workshop, in which case this server
              cannot be joined until its admin updates the mod list.
            </div>
          )}

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
                      onClick={() =>
                        openWorkshopPage(mod.id).catch(() =>
                          onError("Could not open the Workshop page."),
                        )
                      }
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
          {missing > 0 && !downloading && (
            <button
              className="btn"
              onClick={installMissing}
              title={
                download?.unknown.length
                  ? `${download.unknown.length} of these are unknown to Steam and may never download`
                  : undefined
              }
            >
              <Download size={14} />
              Subscribe &amp; install {missing}
              {download && download.bytes > 0 && (
                <span style={{ color: "var(--text-muted)" }}>
                  · {formatBytes(download.bytes)}
                </span>
              )}
            </button>
          )}

          {downloading && (
            <>
              <button className="btn" disabled>
                <Loader2 size={14} className="spin" />
                Installing {progress.done}/{progress.total}
              </button>
              {/* Stopping the wait, not the download: Steam keeps going, and
                  the user gets their interface back either way. */}
              <button
                className="btn btn-ghost"
                onClick={() => {
                  setAwaiting(null);
                  setStalled(false);
                  void load(false);
                }}
              >
                Stop waiting
              </button>
            </>
          )}

          <div style={{ flex: 1 }} />

          <button className="btn" onClick={onClose}>
            Close
          </button>
          <button
            className="btn btn-primary"
            disabled={!ready}
            title={joinTitle}
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
