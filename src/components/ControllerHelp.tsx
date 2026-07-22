// SPDX-License-Identifier: GPL-3.0-or-later
import { Gamepad2 } from "lucide-react";

/**
 * Button labels differ by hardware: the same physical button is A on an Xbox
 * pad, B on a Nintendo one and ✕ on a PlayStation one. Showing "A opens" to
 * someone holding a Pro Controller sends them to the wrong button, so the
 * labels follow whatever is plugged in.
 */
type Family = "nintendo" | "playstation" | "xbox" | "deck" | "generic";

function family(id: string, steamDeck: string | null): Family {
  if (steamDeck) return "deck";
  const s = id.toLowerCase();
  if (s.includes("057e") || s.includes("nintendo") || s.includes("switch"))
    return "nintendo";
  if (s.includes("054c") || s.includes("dualsense") || s.includes("dualshock") ||
      s.includes("playstation") || s.includes("sony"))
    return "playstation";
  if (s.includes("045e") || s.includes("xbox") || s.includes("microsoft"))
    return "xbox";
  return "generic";
}

/** Face buttons, in Gamepad API index order: south, east, west, north. */
const FACE: Record<Family, [string, string, string, string]> = {
  //        south  east   west   north
  nintendo: ["B", "A", "Y", "X"],
  playstation: ["✕", "○", "□", "△"],
  xbox: ["A", "B", "X", "Y"],
  deck: ["A", "B", "X", "Y"],
  generic: ["south", "east", "west", "north"],
};

const SHOULDER: Record<Family, [string, string]> = {
  nintendo: ["L", "R"],
  playstation: ["L1", "R1"],
  xbox: ["LB", "RB"],
  deck: ["L1", "R1"],
  generic: ["L1", "R1"],
};

interface Props {
  padId: string | null;
  steamDeck: string | null;
}

export default function ControllerHelp({ padId, steamDeck }: Props) {
  const kind = family(padId ?? "", steamDeck);
  const [south, east, west, north] = FACE[kind];
  const [l1, r1] = SHOULDER[kind];

  const rows: Array<[string, string]> = [
    ["D-pad / left stick", "Move"],
    [south, "Open the selected server"],
    [east, "Back, or close a dialog"],
    [west, "Toggle favourite"],
    [north, "Join straight away"],
    [`${l1} / ${r1}`, "Previous / next page"],
  ];

  const name = steamDeck ?? padId?.replace(/\s*\(Vendor:.*$/i, "").trim();

  return (
    <section className="section">
      <h3 className="section-title">
        <Gamepad2 size={13} style={{ verticalAlign: -2, marginRight: 5 }} />
        Controller
      </h3>

      <p className="hint">
        {name ? (
          <>
            Detected: <strong>{name}</strong>.
          </>
        ) : (
          "No controller detected. Plug one in and it will appear here."
        )}{" "}
        In Steam Deck's Game Mode, Steam Input turns the controller into
        keystrokes before Balota ever sees it, so the mapping below is handled by
        Steam's own layout there.
      </p>

      <div className="pad-map">
        {rows.map(([button, does]) => (
          <div className="pad-row" key={button}>
            <span className="pad-key">{button}</span>
            <span>{does}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
