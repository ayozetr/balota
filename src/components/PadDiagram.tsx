// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * Controller artwork, from Kenney's Input Prompts (CC0).
 *
 * Hand-drawing these was a losing game: every family has a different
 * silhouette and control layout, and an approximation points people at the
 * wrong place. These are the real shapes.
 *
 * The body is one filled path, so individual buttons cannot be highlighted on
 * it. The per-button glyphs beside each action carry that job instead, and
 * they light up while held — which is what actually settles the mapping,
 * since face-button names are not portable between makers.
 */

export type Family = "nintendo" | "playstation" | "xbox" | "deck" | "generic";

/** Every glyph, loaded as raw markup so `currentColor` can drive its state. */
const ART = import.meta.glob("../assets/controllers/*.svg", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

function art(family: Family, slot: string): string | null {
  // "generic" has no artwork of its own; the Xbox layout is the closest
  // thing to a neutral pad.
  const key = family === "generic" ? "xbox" : family;
  return ART[`../assets/controllers/${key}_${slot}.svg`] ?? null;
}

export function Glyph({
  family,
  slot,
  active,
}: {
  family: Family;
  slot: string;
  active?: boolean;
}) {
  const svg = art(family, slot);
  if (!svg) return null;

  return (
    <span
      className={`pad-glyph${active ? " active" : ""}`}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}

export default function PadBody({ family }: { family: Family }) {
  const svg = art(family, "body");
  if (!svg) return null;

  return (
    <span className="pad-body" dangerouslySetInnerHTML={{ __html: svg }} />
  );
}
