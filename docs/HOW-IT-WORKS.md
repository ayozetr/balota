# How it works

Everything a Linux DayZ launcher has to get right, and why. Most of this is folklore
scattered across forum posts and shell scripts; it is written down here so the next
person does not have to rediscover it.

## 1. Loading mods under Proton

DayZ takes its mods as a command-line argument:

```
-mod=@ModA;@ModB;@ModC
```

Each `@Name` is a folder inside the game directory. Steam, however, downloads Workshop
content somewhere else entirely:

```
steamapps/workshop/content/221100/<workshop_id>/
```

On Windows, DZSA Launcher bridges the gap with junctions. On Linux the equivalent is a
symlink — but the naming matters for two reasons:

1. **Mod names are hostile.** Workshop titles carry spaces, punctuation and non-ASCII
   characters. Passed through Steam's argument handling and then Wine's, they break.
2. **The command line has a limit.** A server with 40 mods produces a very long
   `-mod=` string, and going over the limit means the game silently loads nothing.

The fix is to name each link after the *number*, encoded compactly:

```
1564026768  →  bytes little-endian: 10 5E 27 90 …  →  base64  →  @kCc5XQ
```

Concretely: serialise the ID as a little-endian integer, base64 the bytes, drop the
padding, and swap `/` → `-` and `+` → `_`.

Two details are easy to get wrong. Encoding the *decimal text* (`"1564026768"`) instead
of the integer produces `@MTU2NDAyNjc2OA` — valid, but more than twice as long, which
throws away the reason the encoding exists. And the character swap is **not** standard
URL-safe base64, which maps `+` → `-` and `/` → `_` — the opposite way round.

`src-tauri/src/steam.rs` implements this, and its tests pin the exact output for a set
of known IDs, so a refactor cannot silently change the naming — a wrong link name means
the game loads nothing and the server rejects you, with no error pointing back here.

## 2. Finding the installation

Never assume `~/.local/share/Steam/steamapps/common/DayZ`. In practice:

- Steam lives in `~/.local/share/Steam`, `~/.steam/steam`, `~/.steam/root`,
  `~/.var/app/com.valvesoftware.Steam/data/Steam` (Flatpak) or under `~/snap`. Several
  of those are symlinks to each other, so canonicalise before deduplicating.
- Games are spread across **libraries** listed in `steamapps/libraryfolders.vdf`. A 20 GB
  game very often is not on the system drive — and on a Steam Deck it is usually on the
  SD card.
- The folder name comes from `installdir` in `steamapps/appmanifest_221100.acf`. It is
  normally `DayZ`, but it is not guaranteed.
- Workshop content usually sits in the same library as the game. Usually.

`src-tauri/src/vdf.rs` has a small KeyValues parser for these files.

## 3. Talking to the servers

### DZSA API

```
GET https://dayzsalauncher.com/api/v2/launcher/servers/dayz
```

Returns the whole list — about 21,000 servers, ~26 MB, ~3 MB gzipped, roughly a second —
and every entry already contains its mod list with Workshop IDs. There is no need for
BattleMetrics, the Valve master server, or mass port scanning.

```
GET https://dayzsalauncher.com/api/v1/query/<ip>/<query_port>
```

Same schema for a single server, live. Used just before joining so the mod list is
current.

Both wrap the payload in `{"status": 0, "result": …}`, and report failure as
`{"status": 1, "error": "Timeout has occurred"}` with HTTP 200 — so checking the status
code is not enough.

`steamWorkshopId` arrives as a **JSON number**. Declaring it as a string makes
deserialization fail on every single server.

### Game port vs query port

Each server exposes two different ports and mixing them up is the classic bug:

| Field | Meaning | Used for |
| --- | --- | --- |
| `gamePort` | game traffic, usually 2302 | `-connect=ip:port` |
| `endpoint.port` | query, usually 27016 | the API and A2S |

They are not related by a fixed offset: 2302/27016 is common, but so is 2702/2703.

### A2S over UDP

For real latency, ask the server directly:

```
FF FF FF FF 54 "Source Engine Query\0"
```

Since 2020 the first answer is a **challenge**, not data:

```
FF FF FF FF 41 <4 bytes>        ← 'A', nine bytes total
```

The query has to be sent again with those four bytes appended, and only then does the
info reply arrive:

```
FF FF FF FF 49 <protocol> <name\0> <map\0> <folder\0> <game\0> <appid:u16> <players:u8> <max:u8> …
```

Two traps here:

- Nine bytes back does **not** mean the server answered your question. Treating any
  reply longer than six bytes as success yields a plausible-looking ping with no data
  behind it.
- On DayZ the `game` field is not a game name — admins put their description or Discord
  URL in it — and the AppID field is 16 bits, so DayZ's 221100 does not fit and arrives
  as 0. Parse by walking the fields in order; do not index into the packet by splitting
  on null bytes.

Latency is measured on the first round trip, before the challenge adds a second hop.

## 4. Launching

```
steam -applaunch 221100 -mod=@a;@b -connect=IP:GAMEPORT -nolauncher -world=empty -name=NAME
```

With the Flatpak build:

```
flatpak run --branch=stable --arch=x86_64 --command=/app/bin/steam-wrapper \
  com.valvesoftware.Steam -applaunch 221100 …
```

The Flatpak needs Steam to be running already.

**`steam -applaunch` cannot be wrapped.** It does not start the game: it sends a request
over IPC to the running Steam client, which starts the game as its own child. The CLI
process lives for milliseconds. So `gamemoderun steam -applaunch …` wraps a process that
does nothing — GameMode and MangoHud have to go into Steam's per-game launch options
(`gamemoderun mangohud %command%`) instead.

## 5. Installing mods

### Subscribing (the good path)

Downloading a mod is not the same as subscribing to it. A downloaded item sits on disk
and rots: Steam does not update it, and there is no obvious way to get rid of it.
Subscribing is what users actually want — automatic updates, and one click to drop the
mod when moving to another server.

The client exposes no console command for it, but the Steamworks SDK does:
`ISteamUGC::SubscribeItem()` and `UnsubscribeItem()`. Balota calls them from a separate
`balota-workshop` process, for two reasons:

- Initialising the SDK means announcing **AppID 221100**, so Steam reports the user as
  *playing DayZ* for as long as the process lives. A few seconds is fine; a whole
  session is not.
- A crash inside Valve's library takes down the helper, not the launcher.

`libsteam_api.so` is taken from the user's own Steam install (`steamrt64/`, `linux64/`),
so nothing of Valve's has to be redistributed. The AppImage is the exception: it ships
the redistributable copy next to the helper, because host paths may not be reachable
from inside it.

### Downloading (the fallback)

When the helper cannot run — Steam closed, library missing — the client will still fetch
Workshop items on request, and every ID can go in one invocation:

```bash
steam +workshop_download_item 221100 <id> +workshop_download_item 221100 <id> …
```

It returns immediately, Steam queues the items and downloads them in the background, and
the content lands in `steamapps/workshop/content/221100/<id>` — exactly where the
symlinks need it. No credentials are involved: it reuses the session the client already
has. Under Flatpak, the same arguments go after
`flatpak run … com.valvesoftware.Steam`.

**What not to do.** The obvious alternative is opening
`steam://url/CommunityFilePage/<id>` per mod so the user can hit Subscribe. It looks
reasonable with three mods and falls apart with forty: the pages hit Steam's *web*
frontend, which rate-limits that burst as abuse and answers with a temporary block —
locking the user out of the very pages they need. Spacing the requests out does not fix
it either; it just takes longer to get blocked, and forty tabs is not a workflow anyone
should be handed. Balota opens a Workshop page only when the user clicks one specific
mod.

Items obtained this way are **not subscribed**, so Steam will not keep them up to date;
downloading again is the refresh. Either way Steam reports no completion event, so
progress is tracked by watching the content folder for the IDs that were requested.

Anonymous SteamCMD is not an option for DayZ Workshop content, and asking a launcher's
users for their Steam password is not a trade worth making.
