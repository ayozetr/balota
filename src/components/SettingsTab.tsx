// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import {
  AlertTriangle,
  ClipboardCopy,
  ExternalLink,
  Gauge,
  Github,
  Heart,
  RefreshCw,
  Scale,
} from "lucide-react";
import type { AppConfig, SettingsPatch, SteamEnvironment } from "../types";
import { copyToClipboard, openSteamProperties, openUrl } from "../api";
import { KOFI_URL, LICENSE_URL, REPO_URL } from "../links";
import ControllerHelp from "./ControllerHelp";
import type { PadState } from "../useGamepad";

const LAUNCH_OPTIONS = "gamemoderun mangohud %command%";

interface Props {
  config: AppConfig;
  env: SteamEnvironment | null;
  /** Why the current in-game name will not work, if it will not. */
  nameProblem: string | null;
  onChange: (patch: SettingsPatch) => void;
  onRedetect: () => void;
  onNotice: (message: string) => void;
  onError: (message: string) => void;
  pad: PadState;
}

export default function SettingsTab({
  config,
  env,
  nameProblem,
  onChange,
  onRedetect,
  onNotice,
  onError,
  pad,
}: Props) {
  const [steamRoot, setSteamRoot] = useState(config.customSteamRoot ?? "");
  const [name, setName] = useState(config.playerName);
  const [args, setArgs] = useState(config.extraLaunchArgs);
  const [version, setVersion] = useState("");

  // Hardcoding the version in three places meant one of them was always wrong.
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(""));
  }, []);

  const open = (url: string) =>
    openUrl(url).catch(() => onError("Could not open the link. Is xdg-open installed?"));

  return (
    <div className="panel panel-pad" style={{ flex: 1 }}>
      <section className="section">
        <h3 className="section-title">Player</h3>
        <label className="field" style={{ gap: 12 }}>
          <span style={{ width: 132 }}>In-game name</span>
          <input
            type="text"
            value={name}
            placeholder="Your name"
            className={nameProblem ? "invalid" : undefined}
            aria-invalid={nameProblem ? true : undefined}
            aria-describedby="name-hint"
            onChange={(e) => setName(e.target.value)}
            onBlur={() => name !== config.playerName && onChange({ playerName: name })}
            style={{ width: 240 }}
          />
        </label>
        <p id="name-hint" className={nameProblem ? "hint warn" : "hint"}>
          {nameProblem ??
            "Passed to the game as -name=. Taken from your Steam display name the first time Balota runs."}
        </p>
        {env?.suggestedName && env.suggestedName !== name && (
          <div>
            <button
              className="btn btn-sm"
              onClick={() => {
                setName(env.suggestedName!);
                onChange({ playerName: env.suggestedName! });
              }}
            >
              Use “{env.suggestedName}” from Steam
            </button>
          </div>
        )}
      </section>

      <section className="section">
        <h3 className="section-title">Launch parameters</h3>
        <input
          type="text"
          className="mono"
          value={args}
          placeholder="-cpuCount=8 -noSplash"
          onChange={(e) => setArgs(e.target.value)}
          onBlur={() =>
            args !== config.extraLaunchArgs && onChange({ extraLaunchArgs: args })
          }
        />
        <p className="hint">
          Appended to every launch. Quotes are honoured, so{" "}
          <code>-profiles=&quot;/two words&quot;</code> works. Balota always adds{" "}
          <code>-connect</code>, <code>-mod</code>, <code>-name</code>,{" "}
          <code>-nolauncher</code> and <code>-world=empty</code> on its own.
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
        <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <input
            type="text"
            className="mono"
            readOnly
            value={LAUNCH_OPTIONS}
            aria-label="Steam launch options"
            style={{ flex: 1, minWidth: 220 }}
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
          <button
            className="btn btn-sm"
            onClick={() =>
              openSteamProperties().catch(() => onError("Could not open Steam."))
            }
          >
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

          <dt>Workshop folders</dt>
          <dd className="mono">
            {env?.workshopDirs.length ? env.workshopDirs.join("\n") : "not found"}
          </dd>
        </dl>

        <label className="field" style={{ gap: 12, marginTop: 4 }}>
          <span style={{ width: 132 }}>Custom Steam path</span>
          <input
            type="text"
            className="mono"
            value={steamRoot}
            placeholder="/run/media/deck/SD/Steam"
            onChange={(e) => setSteamRoot(e.target.value)}
            onBlur={() => {
              const next = steamRoot.trim() === "" ? null : steamRoot.trim();
              if (next !== (config.customSteamRoot ?? null)) {
                onChange({ customSteamRoot: next });
              }
            }}
            style={{ flex: 1 }}
          />
        </label>
        <p className="hint">
          Only needed when auto-detection fails. Point it at the folder that contains{" "}
          <code>steamapps</code>; <code>~</code> is expanded. If the path is not usable
          it is ignored and the reason appears above.
        </p>

        <div>
          <button className="btn btn-sm" onClick={onRedetect}>
            <RefreshCw size={13} />
            Detect again
          </button>
        </div>
      </section>

      <ControllerHelp pad={pad} steamDeck={env?.steamDeck ?? null} />

      <section className="section">
        <h3 className="section-title">About</h3>
        {/* GPLv3 §5(d): an interactive program must show this notice. */}
        <p className="hint">
          Balota {version} · Copyright © 2026 Ayoze Torres.
          <br />
          This program comes with ABSOLUTELY NO WARRANTY. It is free software, and you
          are welcome to redistribute it under the terms of the GNU General Public
          License v3 or later, with an additional permission for the Steamworks SDK.
        </p>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button className="btn btn-sm" onClick={() => open(REPO_URL)}>
            <Github size={13} />
            github.com/ayozetr/balota
          </button>
          <button className="btn btn-sm" onClick={() => open(KOFI_URL)}>
            <Heart size={13} />
            Support on Ko-fi
          </button>
          <button className="btn btn-sm" onClick={() => open(LICENSE_URL)}>
            <Scale size={13} />
            Licence
          </button>
        </div>

        <p className="hint" style={{ marginTop: 4 }}>
          Balota is an unofficial tool built by the community. It is not affiliated
          with, authorised by or endorsed by Bohemia Interactive a.s., Valve
          Corporation or DZSA Launcher. Bohemia Interactive, ARMA, DayZ and all
          associated logos and designs are trademarks or registered trademarks of
          Bohemia Interactive a.s. Steam and the Steam logo are trademarks of Valve
          Corporation. All other trademarks are the property of their respective
          owners.
        </p>
      </section>
    </div>
  );
}
