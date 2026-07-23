// SPDX-License-Identifier: GPL-3.0-or-later
import { Gamepad2 } from "lucide-react";
import PadBody, { Glyph } from "./PadDiagram";
import type { Family } from "./PadDiagram";
import type { PadState } from "../useGamepad";

/**
 * Which hardware is in the player's hands, so the artwork and the button
 * glyphs match it. The same physical button is A on an Xbox pad, B on a
 * Nintendo one and ✕ on a PlayStation one, so guessing wrong sends people to
 * the wrong button.
 */
function family(id: string, steamDeck: string | null): Family {
  if (steamDeck) return "deck";
  const s = id.toLowerCase();
  if (s.includes("057e") || s.includes("nintendo") || s.includes("switch"))
    return "nintendo";
  if (
    s.includes("054c") ||
    s.includes("dualsense") ||
    s.includes("dualshock") ||
    s.includes("playstation") ||
    s.includes("sony")
  )
    return "playstation";
  if (s.includes("045e") || s.includes("xbox") || s.includes("microsoft"))
    return "xbox";
  if (s.includes("28de") || s.includes("valve") || s.includes("steam"))
    return "deck";
  return "generic";
}

/** Gamepad API button indices, paired with what Balota does with them. */
const ACTIONS: Array<{ slot: string; index: number | null; does: string }> = [
  { slot: "dpad", index: null, does: "Move through the list" },
  { slot: "south", index: 0, does: "Open the selected server" },
  { slot: "east", index: 1, does: "Back, or close a dialog" },
  { slot: "west", index: 2, does: "Toggle favourite" },
  { slot: "north", index: 3, does: "Join straight away" },
  { slot: "l", index: 4, does: "Previous page" },
  { slot: "r", index: 5, does: "Next page" },
];

const DPAD_BUTTONS = [12, 13, 14, 15];

interface Props {
  pad: PadState;
  steamDeck: string | null;
}

export default function ControllerHelp({ pad, steamDeck }: Props) {
  const kind = family(pad.id ?? "", steamDeck);
  const name = steamDeck ?? pad.id?.replace(/\s*\(Vendor:.*$/i, "").trim();
  const connected = pad.id !== null;
  // An empty mapping means the driver's own button order, which is not
  // guaranteed to match the standard face-button indices the glyphs assume.
  const nonStandard = connected && pad.mapping !== "standard";

  return (
    <section className="section">
      <h3 className="section-title">
        <Gamepad2 size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
        Controller
      </h3>

      <p className="hint">
        {name ? (
          <>
            Detected: <strong>{name}</strong>. Press a button and it lights up
            below.
          </>
        ) : (
          "No controller detected. Plug one in or pair it and it will appear here."
        )}{" "}
        In Steam Deck's Game Mode, Steam Input turns the controller into
        keystrokes before Balota sees it, so there the mapping comes from
        Steam's own layout.
      </p>

      {nonStandard && (
        <div className="note">
          This controller reports a non-standard mapping, common for a Nintendo
          pad over Bluetooth. Navigation still works, but the highlighted glyphs
          may not match the physical button. The raw readout below shows what is
          actually being pressed.
        </div>
      )}

      <div className="pad-layout">
        <PadBody family={kind} />

        <div className="pad-map">
          {ACTIONS.map(({ slot, index, does }) => (
            <div className="pad-row" key={slot}>
              <Glyph
                family={kind}
                slot={slot}
                active={
                  index === null
                    ? pad.pressed.some((b) => DPAD_BUTTONS.includes(b))
                    : pad.pressed.includes(index)
                }
              />
              <span>{does}</span>
            </div>
          ))}
        </div>
      </div>

      {connected && (
        <p className="hint mono" style={{ fontSize: 11 }}>
          mapping: {pad.mapping || "non-standard"} · buttons: {pad.buttonCount} ·
          pressed: [{pad.pressed.join(", ")}] · axes: [
          {pad.axes.map((v) => v.toFixed(2)).join(", ")}]
        </p>
      )}
    </section>
  );
}
