# Roadmap

Ordered by how much each item improves the thing people actually do with a DayZ
launcher: get into a modded server without fighting the tooling.

## Next

- **Mod update detection.** A server rejects you when your copy of a mod is older than
  its own, and downloaded items do not auto-update. Compare the local `meta.cpp`
  timestamp against the Workshop item and offer a re-download, which turns a cryptic
  kick into a clear message.
- **Surface download size before starting.** Forty mods can be several GB; the user
  should see that number before committing to it.
- **Remember the last filters.** Filter state is in-memory only right now.
- **Auto-retry the master list.** One failed request currently leaves the list empty
  until the user presses Refresh.

## After that

- **Gamepad navigation** for Steam Deck Game Mode: focus ring, D-pad, A/B. Balota is
  usable there today, but only with the trackpad.
- **Ping history per favourite**, so a bad night on a server is visible.
- **Server notes**, a local free-text field per server. Nobody remembers which of five
  identically named servers was the good one.
- **Flatpak package** for Flathub as the main distribution channel — it also solves the
  WebKitGTK version drift across distros. The app ID `io.github.ayozetr.Balota` is
  already reserved by convention.

## Explicitly not planned

- **SteamCMD with user credentials.** Automating downloads means asking for a Steam
  password, which breaks on Steam Guard, violates the ToS, and is not something a
  launcher should ever request.
- **Toggles for GameMode/MangoHud.** They cannot work from a launcher that uses
  `steam -applaunch`; see [docs/HOW-IT-WORKS.md](docs/HOW-IT-WORKS.md#4-launching).
  Settings points at Steam's launch options instead.
- **Windows support.** DZSA Launcher already exists there and is better at it.
- **Mass pinging the whole list.** The master list already carries player counts; probing
  21,000 servers to sort a table nobody scrolls to the end of is wasted bandwidth.
