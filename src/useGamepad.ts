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

const DEADZONE = 0.6;
/** Held-down repeat, matching a keyboard's feel. */
const REPEAT_DELAY = 380;
const REPEAT_RATE = 90;

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

export function useGamepad(): string | null {
  const [padId, setPadId] = useState<string | null>(null);

  useEffect(() => {
    if (!("getGamepads" in navigator)) return;

    let frame = 0;
    // Per-key timestamps: when it was first pressed, when it last fired.
    const held = new Map<string, { since: number; last: number }>();

    const poll = () => {
      const pads = Array.from(navigator.getGamepads?.() ?? []).filter(Boolean);
      setPadId(pads[0]?.id ?? null);

      const now = performance.now();
      const pressed = new Set<string>();

      for (const pad of pads) {
        if (!pad) continue;

        pad.buttons.forEach((button, index) => {
          const key = BUTTONS[index];
          if (key && button.pressed) pressed.add(key);
        });

        pad.axes.forEach((value, index) => {
          if (Math.abs(value) < DEADZONE) return;
          const key = AXIS_KEYS[`${index}${value < 0 ? "-" : "+"}`];
          if (key) pressed.add(key);
        });
      }

      for (const key of pressed) {
        const state = held.get(key);
        if (!state) {
          // Fire once on press, then wait before repeating.
          held.set(key, { since: now, last: now });
          send(key);
        } else if (
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

      frame = requestAnimationFrame(poll);
    };

    const onConnect = (e: GamepadEvent) => setPadId(e.gamepad.id);
    window.addEventListener("gamepadconnected", onConnect);
    frame = requestAnimationFrame(poll);

    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("gamepadconnected", onConnect);
    };
  }, []);

  return padId;
}
