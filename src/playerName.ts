// SPDX-License-Identifier: GPL-3.0-or-later

/**
 * Mirrors `steam::name_error` in the backend, so the interface can disable
 * Join and say why *before* the game is launched.
 *
 * The backend is still the one that enforces it — this only exists so the user
 * finds out here, rather than by staring at a frozen "Loading timer" for a
 * minute and then being dropped without a reason.
 */
const PLACEHOLDERS = ["survivor", "player", "unnamed"];

export function playerNameError(name: string | undefined): string | null {
  const trimmed = (name ?? "").trim();

  if (trimmed === "") {
    return "Set an in-game name before joining a server.";
  }
  if (/\s/.test(trimmed)) {
    return "The in-game name cannot contain spaces — DayZ would read the rest as launch parameters. Use an underscore instead.";
  }
  if (/["\u0000-\u001f\u007f]/.test(trimmed)) {
    return "The in-game name cannot contain quotes or control characters.";
  }
  if (PLACEHOLDERS.includes(trimmed.toLowerCase())) {
    return `“${trimmed}” is DayZ's placeholder name and a lot of servers kick it on sight. Pick your own name.`;
  }
  return null;
}
