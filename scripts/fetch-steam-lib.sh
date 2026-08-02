#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Copies Valve's libsteam_api.so out of the steamworks-sys crate so the
# Workshop helper can be bundled with it.
#
# This replaces a one-liner that lived in package.json:
#
#   cp "$(find $HOME/.cargo/registry/src \
#          -path '*steamworks-sys*/redistributable_bin/linux64/libsteam_api.so' \
#          | head -1)" binaries/
#
# Four things were wrong with it, and this script fixes each one:
#
#   1. `-path` wildcards cross `/` in GNU find, so `*steamworks-sys*` was not
#      anchored to a crate directory. Any crate whose tarball happened to
#      contain `vendor/steamworks-sys/redistributable_bin/linux64/libsteam_api.so`
#      matched — and `cargo fetch` extracts a crate without running its
#      build.rs, so nothing has to execute for those bytes to land on disk.
#      Verified: the legitimate path is
#      `steamworks-sys-0.13.0/lib/steam/redistributable_bin/linux64/…`, which
#      only matched because the wildcard spanned two path components.
#   2. `head -1` picked by readdir order, so two versions in the registry meant
#      a coin toss and no error.
#   3. `$HOME/.cargo` was hardcoded, ignoring CARGO_HOME — which the README
#      itself suggests changing by recommending a container build.
#   4. When find matched nothing, `cp ""` failed but the build carried on,
#      leaving a bundle with the helper and no library.
#
# The expected hash is pinned in the repo so the bytes being shipped are the
# bytes that were reviewed. Update it deliberately, in a commit of its own,
# when steamworks-sys is bumped.

set -euo pipefail

EXPECTED_SHA=ec4797f76a206eb0af627af0f0788eb5a2eabf5dee38ee0a6affbaa44a645f4e

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
lockfile="$repo_root/src-tauri/Cargo.lock"
dest="$repo_root/src-tauri/binaries"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"

[[ -f "$lockfile" ]] || { echo "error: no Cargo.lock at $lockfile" >&2; exit 1; }

# The version that is actually locked, not whatever happens to be cached.
version=$(awk '
  /^name = "steamworks-sys"$/ { found = 1; next }
  found && /^version = / { gsub(/[",]/, "", $3); print $3; exit }
' "$lockfile")

if [[ -z "$version" ]]; then
  echo "error: steamworks-sys is not in Cargo.lock" >&2
  exit 1
fi

# -name on a directory component: no wildcard that can span a '/'.
mapfile -t crates < <(
  find "$cargo_home/registry/src" \
    -mindepth 2 -maxdepth 2 \
    -type d -name "steamworks-sys-$version" \
    2>/dev/null | sort
)

if (( ${#crates[@]} == 0 )); then
  echo "error: steamworks-sys-$version is not in $cargo_home/registry/src." >&2
  echo "       Run 'cargo fetch --manifest-path src-tauri/Cargo.toml' first." >&2
  exit 1
fi
if (( ${#crates[@]} > 1 )); then
  echo "error: ${#crates[@]} copies of steamworks-sys-$version found; refusing to guess:" >&2
  printf '       %s\n' "${crates[@]}" >&2
  exit 1
fi

so="${crates[0]}/lib/steam/redistributable_bin/linux64/libsteam_api.so"
[[ -f "$so" ]] || { echo "error: $so does not exist" >&2; exit 1; }

actual=$(sha256sum "$so" | cut -d' ' -f1)
if [[ "$actual" != "$EXPECTED_SHA" ]]; then
  echo "error: libsteam_api.so does not match the pinned hash." >&2
  echo "       expected $EXPECTED_SHA" >&2
  echo "       got      $actual" >&2
  echo "       from     $so" >&2
  echo "       If steamworks-sys was updated on purpose, review the new binary" >&2
  echo "       and update EXPECTED_SHA in this script in its own commit." >&2
  exit 1
fi

install -Dm644 "$so" "$dest/libsteam_api.so"
echo "libsteam_api.so ($version, sha256 ${actual:0:12}…) -> $dest"
