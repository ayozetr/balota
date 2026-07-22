// SPDX-License-Identifier: GPL-3.0-or-later
export function formatBytes(bytes: number): string {
  if (!bytes) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

export function timeAgo(epochSeconds: number): string {
  if (!epochSeconds) return "never";
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - epochSeconds);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

export function pingClass(ping: number | null | undefined): string {
  if (ping == null) return "ping-none";
  if (ping < 60) return "ping-good";
  if (ping < 140) return "ping-ok";
  return "ping-bad";
}

export const formatNumber = (value: number) => value.toLocaleString("en-US");
