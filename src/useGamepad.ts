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
 * Keys that stand for "activate this". A synthetic KeyboardEvent is not
 * trusted, so the browser never performs the default action for it: dispatching
 * Enter at a `<button>` moves nothing. Anything with its own `onKeyDown` still
 * gets first refusal, and only if it declines do we click.
 */
const ACTIVATION_KEYS = new Set(["Enter", " "]);
const ACTIVATABLE =
  'button, input[type="checkbox"], input[type="radio"], a[href], [role="button"], [role="option"]';
/** Only these repeat while held. Repeating "open" or "join" is never wanted. */
const REPEATABLE = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown"]);

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

/** The element the pad should act on, or null if there is nothing to act on. */
function padTarget(): HTMLElement | null {
  const active = document.activeElement as HTMLElement | null;
  const dialog = document.querySelector<HTMLElement>('[role="dialog"]');

  // A dialog owns the pad while it is open, even if focus escaped it.
  if (dialog && (!active || !dialog.contains(active))) {
    return (
      dialog.querySelector<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? dialog
    );
  }

  if (active && active !== document.body && active !== document.documentElement) {
    return active;
  }

  // Nothing focused: adopt the first row, or the first control on the page.
  // Looking only for `.row-item` left the pad dead on the Mods and Settings
  // tabs, which have no rows at all.
  return document.querySelector<HTMLElement>(
    '.row-item, .nav-item, button:not([disabled]), input:not([disabled])',
  );
}

function send(key: string) {
  // Synthetic events are not "trusted", so :focus-visible never triggers and
  // the ring stays invisible even though focus moved. Flagging the document
  // lets the stylesheet fall back to plain :focus while a pad is in use.
  document.documentElement.classList.add("using-gamepad");

  let target = padTarget();
  if (!target) return;

  if (target !== document.activeElement) {
    target.focus();
    // Adopting focus is the whole action for a direction key; sending it as
    // well would skip the row the user just landed on.
    if (!ACTIVATION_KEYS.has(key)) return;
  }

  const init: KeyboardEventInit = { key, bubbles: true, cancelable: true };
  const notCancelled = target.dispatchEvent(new KeyboardEvent("keydown", init));
  target.dispatchEvent(new KeyboardEvent("keyup", init));

  // The event was not handled, so nothing happened: an untrusted key event
  // never performs a click on its own. Do it explicitly, or every button in
  // every dialog stays unreachable from the pad.
  if (notCancelled && ACTIVATION_KEYS.has(key) && target.matches(ACTIVATABLE)) {
    target.click();
  }
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
    let running = false;
    const held = new Map<string, { since: number; last: number }>();
    // Axis 9 only counts as a hat once it has been seen resting outside
    // [-1, 1]. Tracked per pad: swapping controllers used to leave the flag
    // armed and an analog axis resting at 0 then decoded as a held "down".
    let hatArmedFor: string | null = null;

    const anyPad = () =>
      Array.from(navigator.getGamepads?.() ?? []).some((p) => p !== null);

    const stop = () => {
      running = false;
      cancelAnimationFrame(frame);
      held.clear();
      hatArmedFor = null;
      setState((s) => (s.id === null ? s : EMPTY));
    };

    const poll = () => {
      const pad = activePad(Array.from(navigator.getGamepads?.() ?? []));

      if (!pad) {
        // No pad at all: stop burning a frame callback until one shows up.
        // This used to poll at 60-90 Hz forever, on battery, on a Deck.
        stop();
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
        if (Math.abs(hat) > 1.05) hatArmedFor = pad.id;
        if (hatArmedFor === pad.id) {
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
        } else if (
          REPEATABLE.has(key) &&
          now - state.since > REPEAT_DELAY &&
          now - state.last > REPEAT_RATE
        ) {
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

    const start = () => {
      if (running) return;
      running = true;
      frame = requestAnimationFrame(poll);
    };

    const onDisconnected = () => {
      // Another pad may still be attached.
      if (!anyPad()) stop();
    };

    // A real key press or click means the pad is not driving any more, so the
    // focus ring goes back to normal. This class used to be added once and
    // never removed, leaving the ring stuck on for the rest of the session.
    const onHumanInput = (e: Event) => {
      if (e.isTrusted) document.documentElement.classList.remove("using-gamepad");
    };

    window.addEventListener("gamepadconnected", start);
    window.addEventListener("gamepaddisconnected", onDisconnected);
    window.addEventListener("pointerdown", onHumanInput, true);
    window.addEventListener("keydown", onHumanInput, true);

    // A pad connected before this mounted does not fire the event again.
    if (anyPad()) start();

    return () => {
      window.removeEventListener("gamepadconnected", start);
      window.removeEventListener("gamepaddisconnected", onDisconnected);
      window.removeEventListener("pointerdown", onHumanInput, true);
      window.removeEventListener("keydown", onHumanInput, true);
      cancelAnimationFrame(frame);
      running = false;
    };
  }, []);

  return state;
}
