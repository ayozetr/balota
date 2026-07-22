import { useState } from "react";
import {
  AlertTriangle,
  ClipboardCopy,
  ExternalLink,
  Gauge,
  RefreshCw,
} from "lucide-react";
import type { AppConfig, SteamEnvironment } from "../types";
import { copyToClipboard, openSteamProperties } from "../api";

const LAUNCH_OPTIONS = "gamemoderun mangohud %command%";

interface Props {
  config: AppConfig;
  env: SteamEnvironment | null;
  onChange: (config: AppConfig) => void;
  onRedetect: () => void;
  onNotice: (message: string) => void;
}

export default function SettingsTab({
  config,
  env,
  onChange,
  onRedetect,
  onNotice,
}: Props) {
  const [steamRoot, setSteamRoot] = useState(config.customSteamRoot ?? "");

  return (
    <div className="panel panel-pad" style={{ flex: 1 }}>
      <section className="section">
        <h3 className="section-title">Player</h3>
        <label className="field" style={{ gap: 12 }}>
          <span style={{ width: 132 }}>In-game name</span>
          <input
            type="text"
            value={config.playerName}
            placeholder="Survivor"
            onChange={(e) => onChange({ ...config, playerName: e.target.value })}
            style={{ width: 240 }}
          />
        </label>
        <p className="hint">
          Passed to the game as <code>-name=</code>. Some community servers refuse
          connections without one.
        </p>
      </section>

      <section className="section">
        <h3 className="section-title">Launch parameters</h3>
        <input
          type="text"
          className="mono"
          value={config.extraLaunchArgs}
          placeholder="-cpuCount=8 -noSplash"
          onChange={(e) => onChange({ ...config, extraLaunchArgs: e.target.value })}
        />
        <p className="hint">
          Appended to every launch. Balota always adds <code>-connect</code>,{" "}
          <code>-mod</code>, <code>-nolauncher</code> and <code>-world=empty</code> on
          its own.
        </p>
      </section>

      <section className="section">
        <h3 className="section-title">
          <Gauge size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
          GameMode and MangoHud
        </h3>
        <p className="hint">
          These cannot be applied from here. Balota starts the game through{" "}
          <code>steam -applaunch</code>, which only hands the request to the running
          Steam client — the game is spawned by Steam itself and never inherits a
          wrapper process. The one place that works is Steam's own launch options:
        </p>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            type="text"
            className="mono"
            readOnly
            value={LAUNCH_OPTIONS}
            style={{ flex: 1 }}
            onFocus={(e) => e.currentTarget.select()}
          />
          <button
            className="btn btn-sm"
            onClick={async () => {
              const ok = await copyToClipboard(LAUNCH_OPTIONS);
              onNotice(ok ? "Copied to the clipboard." : "Could not copy — select and copy by hand.");
            }}
          >
            <ClipboardCopy size={13} />
            Copy
          </button>
          <button className="btn btn-sm" onClick={() => openSteamProperties()}>
            <ExternalLink size={13} />
            Open DayZ properties
          </button>
        </div>
        <p className="hint">
          Paste it into <em>Properties → General → Launch options</em>. Drop{" "}
          <code>gamemoderun</code> or <code>mangohud</code> if you only want one of
          them.
        </p>
      </section>

      <section className="section">
        <h3 className="section-title">Steam installation</h3>

        {env?.notes.map((note, index) => (
          <div className="note" key={index}>
            <AlertTriangle size={15} style={{ flexShrink: 0, marginTop: 1 }} />
            <span>{note}</span>
          </div>
        ))}

        <dl className="kv">
          <dt>Steam root</dt>
          <dd className="mono">{env?.steamRoot ?? "not found"}</dd>

          <dt>Packaging</dt>
          <dd>{env?.isFlatpak ? "Flatpak" : "Native"}</dd>

          <dt>Steam running</dt>
          <dd>{env?.steamRunning ? "yes" : "no"}</dd>

          <dt>Libraries</dt>
          <dd className="mono">
            {env?.libraries.length ? env.libraries.join("\n") : "—"}
          </dd>

          <dt>DayZ folder</dt>
          <dd className="mono">{env?.dayzDir ?? "not found"}</dd>

          <dt>Workshop folder</dt>
          <dd className="mono">{env?.workshopDir ?? "not found"}</dd>
        </dl>

        <label className="field" style={{ gap: 12, marginTop: 4 }}>
          <span style={{ width: 132 }}>Custom Steam path</span>
          <input
            type="text"
            className="mono"
            value={steamRoot}
            placeholder="/run/media/deck/SD/Steam"
            onChange={(e) => setSteamRoot(e.target.value)}
            onBlur={() =>
              onChange({
                ...config,
                customSteamRoot: steamRoot.trim() === "" ? null : steamRoot.trim(),
              })
            }
            style={{ flex: 1 }}
          />
        </label>
        <p className="hint">
          Only needed when auto-detection fails. Point it at the folder that contains{" "}
          <code>steamapps</code>.
        </p>

        <div>
          <button className="btn btn-sm" onClick={onRedetect}>
            <RefreshCw size={13} />
            Detect again
          </button>
        </div>
      </section>
    </div>
  );
}
