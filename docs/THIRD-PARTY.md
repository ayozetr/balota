# Third-party licences

Balota is distributed under the GNU General Public License v3.0 or later, with an
additional permission for the Steamworks SDK (see [LICENSE](../LICENSE)). It builds on
the work below.

Every dependency listed here carries a permissive licence, and all of them are
compatible with the GPL. Regenerate this file with `cargo metadata` and the package
manifests under `node_modules` after changing dependencies.

## The Steamworks SDK

`libsteam_api.so` is **proprietary software owned by Valve Corporation** and is not
covered by Balota's licence. It is used through the `steamworks` crate to subscribe and
unsubscribe Workshop items.

Balota reads it from the user's own Steam installation, so building from source and the
`.deb` package redistribute nothing of Valve's. The AppImage is the exception: it ships
Valve's redistributable copy next to the helper binary, which is what the additional
permission in LICENSE exists to allow.

## Rust crates

462 crates, direct and transitive:

| Licence | Crates |
| --- | ---: |
| MIT OR Apache-2.0 | 225 |
| MIT | 102 |
| Apache-2.0 OR MIT | 31 |
| MIT/Apache-2.0 | 20 |
| Unicode-3.0 | 18 |
| Zlib OR Apache-2.0 OR MIT | 17 |
| MPL-2.0 | 5 |
| Unlicense OR MIT | 4 |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 3 |
| Apache-2.0/MIT | 3 |
| BSD-3-Clause | 3 |
| ISC | 3 |
| MIT OR Apache-2.0 OR Zlib | 3 |
| Apache-2.0 | 2 |
| Apache-2.0 OR ISC OR MIT | 2 |
| BSD-3-Clause OR MIT OR Apache-2.0 | 2 |
| MIT / Apache-2.0 | 2 |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 2 |
| Unlicense/MIT | 2 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 1 |
| 0BSD OR MIT OR Apache-2.0 | 1 |
| Apache-2.0 / MIT | 1 |
| Apache-2.0 AND ISC | 1 |
| Apache-2.0 AND MIT | 1 |
| Apache-2.0 OR BSL-1.0 | 1 |
| Apache-2.0 WITH LLVM-exception | 1 |
| BSD-3-Clause AND MIT | 1 |
| BSD-3-Clause/MIT | 1 |
| CC0-1.0 OR MIT-0 OR Apache-2.0 | 1 |
| CDLA-Permissive-2.0 | 1 |
| MIT OR Zlib OR Apache-2.0 | 1 |
| Zlib | 1 |

## npm packages

74 packages:

| Licence | Packages |
| --- | ---: |
| MIT | 61 |
| ISC | 6 |
| Apache-2.0 OR MIT | 3 |
| Apache-2.0 | 2 |
| BSD-3-Clause | 1 |
| CC-BY-4.0 | 1 |

## Notable direct dependencies

| Project | Licence | Used for |
| --- | --- | --- |
| Tauri | MIT OR Apache-2.0 | Application shell and IPC |
| React | MIT | User interface |
| Lucide | ISC | Icons |
| tokio | MIT | Async runtime, UDP sockets |
| reqwest | MIT OR Apache-2.0 | HTTP client |
| serde | MIT OR Apache-2.0 | Serialisation |
| steamworks (Noxime) | MIT OR Apache-2.0 | Steamworks SDK bindings |

## Services

Server data comes from the public API at dayzsalauncher.com. Balota is not affiliated
with it.

