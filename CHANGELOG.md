# Changelog

All notable changes to Balota are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Everything below came out of a full audit of the 1.0.0 codebase. The headline
is the in-game name: the default was `Survivor`, which is exactly the name a
good number of DayZ servers kick on sight — and neither the game nor the
launcher told you that was what had happened.

### Added

- **The in-game name is taken from Steam.** On first run Balota reads the
  display name of the account that logged in last and uses it, replacing spaces
  with underscores so DayZ does not read the rest as launch parameters. If
  nothing can be detected the field is left empty, and joining is blocked with
  an explanation until a name is set. A config still carrying `Survivor` from an
  earlier version is migrated automatically; a name you typed yourself is never
  overwritten.
- **Download size before subscribing.** The Subscribe button now says how many
  gigabytes are about to arrive, and warns up front about mods Steam does not
  recognise — the ones that would otherwise leave the download stuck forever.
- **Live player counts.** The table shows the numbers measured by the A2S probe
  rather than the master list's, which can be hours old. The tooltip explains
  that the empty/full filters still run over the list data.
- **Confirmation before removing mods**, listing what will be unsubscribed and
  what will be deleted from disk, with the freed space.
- **History entries can be deleted**, so a server that no longer exists stops
  sitting in the list.
- **Filters, sort order and the open tab are remembered** between sessions.
- **Automatic retry** when the server list cannot be reached, with a 2s/5s
  backoff — the usual failure is the app starting before Wi-Fi has associated.
- **A release workflow** that builds the bundles in CI and publishes
  `SHA256SUMS` alongside them.

### Fixed

- **Mods on disk were reported as missing.** The Workshop folder was located
  once at startup and cached for the session, so anyone installing their first
  mod, or keeping mods in a second Steam library, could never launch. It is now
  resolved at the moment of use, across every library.
- **Controller buttons did nothing outside the server table.** Synthetic key
  events are untrusted, so they never activate a `<button>`; the pad now clicks
  the focused control when nothing else handles the key. Reaching the Mods and
  Settings tabs with a pad works too.
- **"Loading servers…" forever** when the first list download failed. There is
  now an error state with a retry button.
- **"Installing X/N" stuck forever** when Steam accepted only some of the mods.
  The backend reports which IDs it actually took, the dialog waits only for
  those, and there is a stall warning and a way out.
- **Favourites and history could be silently lost.** Settings sent the whole
  config back on every keystroke, so a name typed in the header could undo a
  favourite starred a moment earlier; saving is now a patch applied under a
  single lock, written with `O_EXCL`, `0600` and `fsync`.
- **A single malformed server record made the entire list unreadable**, cache
  included. Records are now dropped individually.
- **Servers were shown as offline after one dropped UDP packet.** The A2S query
  retries properly, and a split reply reports the server as online instead of
  discarding a latency it had already measured.
- **The sort arrow lied.** Clicking a column header now actually reverses the
  order, and `aria-sort` reflects it.
- **Page buttons lost clicks** when pressed twice while a query was in flight.
- **Keyboard shortcuts reached through open dialogs**, paging the list behind
  the overlay and breaking the focus trap.
- **"Direct connect" could not be closed** with Escape or the pad's B button.
- **`999.999.999.999:99999` was accepted** as a direct-connect address.
- **The custom Steam path was discarded in silence** when it was not usable,
  and `~` was not expanded.
- **Steam libraries went missing** if `libraryfolders.vdf` started with a BOM
  or contained a key with no value; deeply nested input could crash the app
  outright.
- **`meta.cpp`, mod folder sizes and the server list had no size limits**, and
  `dir_size` followed a symlinked mod folder out of the Workshop directory.
- **"Clean up links" could delete symlinks the user made by hand**, including
  when the target only looked missing because a drive was unmounted.
- **Deleting mods stopped at the first failure** and reported nothing.
- **Zombie processes** were left behind by every game launch and URL opened.
- **`LD_LIBRARY_PATH` was replaced rather than extended**, breaking the
  Workshop helper when Balota was started from Steam.
- The window asked for 1360×860 on a Steam Deck's 1280×800 screen, the
  stylesheet had no media queries at all, and `--text-dim` sat at 3.5:1
  contrast — below WCAG AA — at 11px.
- The declared MSRV was wrong: the dependency tree requires Rust 1.88, not 1.77.

### Security

- **`endpoint.ip` is validated as a real IP address** at the parser. It reaches
  the game's command line, a DNS lookup and a URL path, and was previously
  taken on trust from a third-party API. `-connect=` is parsed as a
  `SocketAddr` and the lookup URL is built from path segments.
- **The bundled `libsteam_api.so` is pinned by sha256.** It used to be picked
  with `find -path '*steamworks-sys*/…' | head -1`, whose wildcard crosses `/`
  — so any crate shipping that path inside its tarball would match, without
  needing to execute anything.
- **Response bodies are capped at 64 MB.** With gzip enabled, the timeout said
  nothing about how much memory a body could become, and an oversized one was
  written to the cache and re-read on every start.
- **`installdir` from the app manifest is validated.** An absolute path there
  made `Path::join` discard the prefix entirely, pointing the game folder
  anywhere on disk.
- **Deletion guards are real.** `Path::starts_with` is textual and never
  rejects a `..`; paths are now canonicalised.
- **Settings are written atomically to a per-process temporary file** opened
  with `O_EXCL` at mode 0600, in a 0700 directory.
- CSP gained `base-uri`, `form-action`, `object-src` and `frame-*`; capabilities
  no longer pull in `core:default`; the HTTP client is HTTPS-only and follows no
  redirects; `open_url` checks the host, not just the scheme.

### Changed

- CI actually runs now, covers the Workshop helper (`--workspace
  --all-targets --locked -D warnings`), type-checks the frontend and audits
  dependencies. It had been disabled, and all 24 of its historical runs had
  failed at startup without executing a single step.
- Launch parameters honour quotes, so `-profiles="/two words"` works.
- The version shown in Settings comes from the app rather than a hardcoded
  string.
- 25 → 50 tests.

## [1.0.0] — 2026-07-23

First public release.

- Browse every DayZ server from the DZSA master list, with filters for map,
  mods, perspective, password and occupancy.
- Real latency measured over A2S, on the visible page only.
- Resolve the mods a server requires, subscribe to them through the Steam
  client and create the symlinks Proton needs to load them.
- Manage installed Workshop mods, including the untracked leftovers a plain
  download leaves behind.
- Favourites, history and direct connect.
- Full keyboard and controller navigation, with Steam Deck support.
- AppImage and `.deb` bundles.

[Unreleased]: https://github.com/ayozetr/balota/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/ayozetr/balota/releases/tag/v1.0.0
