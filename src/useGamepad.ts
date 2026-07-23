// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";

/**
 * Drives the interface from a physical controller.
 *
 * Rather than a second set of handlers, each button is translated into the
 * key the interface already understands, and the event is dispatched at the
 * focused element. Everything keyboard navigation does — moving through rows,
 * opening a server, closing a dialog, paging — works from the pad for free,
 * and the two paths can never drift apart.
 *
 * Under Steam Input the controller already arrives as keystrokes and this hook
 * simply sees no gamepad, which is the correct outcome: no double input.
 */

/** Standard Gamepad mapping — button index to the key it stands in for. */
const BUTTONS: Record<number, string> = {
  0: "Enter", // A / south: open
  1: "Escape", // B / east: back
  2: "f", // X / west: favourite
  3: "j", // Y / north: join
  4: "PageUp", // L1
  5: "PageDown", // R1
  12: "ArrowUp",
  13: "ArrowDown",
  14: "ArrowLeft",
  15: "ArrowRight",
};

/** Left stick, treated as a d-pad. */
const AXIS_KEYS: Record<string, string> = {
  "1-": "ArrowUp",
  "1+": "ArrowDown",
  "0-": "ArrowLeft",
  "0+": "ArrowRight",
};

/** Direction → (navigation key, virtual d-pad button index for the diagram). */
const HAT_DIRS: Record<string, { key: string; button: number }> = {
  up: { key: "ArrowUp", button: 12 },
  down: { key: "ArrowDown", button: 13 },
  left: { key: "ArrowLeft", button: 14 },
  right: { key: "ArrowRight", button: 15 },
};

const DEADZONE = 0.6;
/** Held-down repeat, matching a keyboard's feel. */
const REPEAT_DELAY = 380;
const REPEAT_RATE = 90;

/**
 * Decodes a d-pad reported as a hat on an axis.
 *
 * Non-standard pads — a Nintendo Pro Controller over `hid-nintendo` is the
 * usual one — put the d-pad on `axes[9]` instead of buttons 12-15, encoded as
 * eight steps from -1 (up) to 1 (up-left), resting above 1 when centred. The
 * eight canonical values are ~0.2857 apart, so rounding to the nearest sector
 * recovers the direction.
 */
export function hatDirections(v: number): string[] {
  // A real hat rests outside [-1, 1] (≈1.29); an analog axis rests at 0, so
  // only decode once the caller has confirmed this axis behaves like a hat.
  if (v < -1.05 || v > 1.05) return [];
  const sector = Math.round((v + 1) / (2 / 7));
  const table: string[][] = [
    ["up"],
    ["up", "right"],
    ["right"],
    ["down", "right"],
    ["down"],
    ["down", "left"],
    ["left"],
    ["up", "left"],
  ];
  return table[sector] ?? [];
}

function send(key: string) {
  // Synthetic events are not "trusted", so :focus-visible never triggers and
  // the ring stays invisible even though focus moved. Flagging the document
  // lets the stylesheet fall back to plain :focus while a pad is in use.
  document.documentElement.classList.add("using-gamepad");

  let target = document.activeElement as HTMLElement | null;

  // With nothing focused the events would go nowhere, so the pad adopts the
  // first row and starts from there.
  if (!target || target === document.body || target === document.documentElement) {
    target = document.querySelector<HTMLElement>(".row-item");
    target?.focus();
    if (!target) return;
  }

  const init: KeyboardEventInit = { key, bubbles: true, cancelable: true };
  target.dispatchEvent(new KeyboardEvent("keydown", init));
  target.dispatchEvent(new KeyboardEvent("keyup", init));
}

/** Among the connected pads, the one actually worth reading. */
function activePad(pads: (Gamepad | null)[]): Gamepad | null {
  const usable = pads.filter((p): p is Gamepad => !!p && p.buttons.length >= 4);
  if (usable.length === 0) return null;
  // A Pro Controller exposes a motion sensor as a second, button-less device;
  // preferring a standard mapping (and, failing that, the most buttons) keeps
  // the real pad rather than the IMU.
  return (
    usable.find((p) => p.mapping === "standard") ??
    usable.sort((a, b) => b.buttons.length - a.buttons.length)[0]
  );
}

export interface PadState {
  id: string | null;
  /** "standard" or "" — an empty mapping means the indices are the driver's. */
  mapping: string;
  /** Button indices held right now, for the on-screen diagram. */
  pressed: number[];
  /** Raw axis values, rounded, for the Settings diagnostic. */
  axes: number[];
  buttonCount: number;
}

const EMPTY: PadState = {
  id: null,
  mapping: "",
  pressed: [],
  axes: [],
  buttonCount: 0,
};

export function useGamepad(): PadState {
  const [state, setState] = useState<PadState>(EMPTY);

  useEffect(() => {
    if (!("getGamepads" in navigator)) return;

    let frame = 0;
    const held = new Map<string, { since: number; last: number }>();
    // Axis 9 only counts as a hat once it has been seen resting outside
    // [-1, 1]; this keeps an analog axis that rests at 0 from firing "down".
    let hatArmed = false;

    const poll = () => {
      const pad = activePad(Array.from(navigator.getGamepads?.() ?? []));

      if (!pad) {
        held.clear();
        setState((s) => (s.id === null ? s : EMPTY));
        frame = requestAnimationFrame(poll);
        return;
      }

      const now = performance.now();
      const pressed = new Set<string>();
      const down: number[] = [];

      pad.buttons.forEach((button, index) => {
        if (!button.pressed) return;
        down.push(index);
        const key = BUTTONS[index];
        if (key) pressed.add(key);
      });

      pad.axes.forEach((value, index) => {
        if (Math.abs(value) < DEADZONE) return;
        const key = AXIS_KEYS[`${index}${value < 0 ? "-" : "+"}`];
        if (key) pressed.add(key);
      });

      // D-pad reported as a hat on axis 9 (non-standard pads).
      const hat = pad.axes[9];
      if (hat !== undefined) {
        if (Math.abs(hat) > 1.05) hatArmed = true;
        if (hatArmed) {
          for (const dir of hatDirections(hat)) {
            const { key, button } = HAT_DIRS[dir];
            pressed.add(key);
            if (!down.includes(button)) down.push(button);
          }
        }
      }

      for (const key of pressed) {
        const state = held.get(key);
        if (!state) {
          held.set(key, { since: now, last: now });
          send(key);
        } else if (now - state.since > REPEAT_DELAY && now - state.last > REPEAT_RATE) {
          state.last = now;
          send(key);
        }
      }
      for (const key of held.keys()) {
        if (!pressed.has(key)) held.delete(key);
      }

      const axes = pad.axes.map((v) => Math.round(v * 100) / 100);
      down.sort((a, b) => a - b);

      setState((previous) => {
        const same =
          previous.id === pad.id &&
          previous.buttonCount === pad.buttons.length &&
          previous.pressed.length === down.length &&
          previous.pressed.every((b, i) => b === down[i]) &&
          previous.axes.length === axes.length &&
          previous.axes.every((v, i) => v === axes[i]);
        return same
          ? previous
          : {
              id: pad.id,
              mapping: pad.mapping,
              pressed: down,
              axes,
              buttonCount: pad.buttons.length,
            };
      });

      frame = requestAnimationFrame(poll);
    };

    frame = requestAnimationFrame(poll);
    return () => cancelAnimationFrame(frame);
  }, []);

  return state;
}
