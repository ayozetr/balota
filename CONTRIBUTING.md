# Contributing

## Setup

Install the system dependencies listed in the [README](README.md), plus Rust 1.77+ and
Node.js 18+.

```bash
npm install
npm run app          # dev mode: Vite on :1420 + the Tauri window, both hot-reloading
```

## Packaging

```bash
npm run app:build     # .AppImage + .deb in src-tauri/target/release/bundle/
```

That script exports two variables, and both are there for a reason:

- `APPIMAGE_EXTRACT_AND_RUN=1` — `linuxdeploy` and `appimagetool` are themselves
  AppImages and try to mount through FUSE. Where FUSE is unavailable (containers, CI,
  sandboxes) they die instantly and Tauri reports only `failed to run linuxdeploy`,
  with no cause. This makes them extract and run instead.
- `NO_STRIP=1` — sidesteps `linuxdeploy`'s stripping step, another source of opaque
  failures.

The `.deb` bundler practically never fails; the AppImage step is the fragile one. To
debug it:

```bash
APPIMAGE_EXTRACT_AND_RUN=1 npx tauri build --bundles appimage --verbose
```

If `linuxdeploy` starts pulling libraries out of an unrelated path (VMware ships ancient
GTK copies under `/usr/lib/vmware/lib`, which then need obsolete dependencies like
`libcroco`), the culprit is `linuxdeploy-plugin-gtk`'s recursive `find`. Patch its
`find` to exclude that path in `~/.cache/tauri/linuxdeploy-plugin-gtk.sh` — and note
that Tauri silently re-downloads the plugin whenever the cache is clean, reverting the
patch.

**The Wayland fix lives in the binary.** `main.rs` sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1` at startup unless the user already set it, so every
bundle boots on Wayland without anyone exporting anything.

## Layout

```
src-tauri/src/
  main.rs      Tauri commands and shared state. The IPC surface, nothing else.
  steam.rs     Installation discovery, mod symlinks, launching.
  dzsa.rs      dayzsalauncher.com client and API types.
  a2s.rs       Valve UDP query, including the challenge handshake.
  servers.rs   Master-list store: disk cache, filtering, sorting, pagination.
  config.rs    ~/.config/balota/config.json
  vdf.rs       Minimal KeyValues parser for Steam's own files.

src/
  App.tsx           State, tabs, the loading pipeline.
  api.ts            Typed wrappers over invoke(). No component calls invoke directly.
  types.ts          Mirrors the Rust payloads (camelCase, as serde emits them).
  components/       One file per view.
```

**Filtering happens in Rust.** With ~21,000 servers, sending the whole list to the
frontend on every keystroke locks the UI. `query_servers` takes a filter and returns one
page.

**Only the visible page is pinged.** 50 UDP probes per page, capped at 64 in flight.
Never ping the full list.

## Tests

```bash
cd src-tauri && cargo test      # 23 tests, no network access needed
npm run build                   # tsc + vite, catches type errors
```

The suite covers the parts that are easy to get subtly wrong and hard to notice: the
mod link encoding (pinned to known-good output), the A2S parser against a captured real
reply, the VDF parser, the API's number-or-string mod IDs, and the filter/pagination
logic.

If you touch any of those, the test comes with the change. A wrong symlink name or a
misparsed A2S packet fails silently at runtime — that is exactly why they are pinned.

## Conventions

- Code, comments, UI strings and docs in English.
- Errors that reach the user say what failed *and* what to do about it. `SteamEnvironment.notes`
  exists so "DayZ not found" can list every path that was searched.
- No dependency gets added to dodge twenty lines of code.
