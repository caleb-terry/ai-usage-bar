#!/usr/bin/env bash
#
# Cargo `runner` for macOS dev builds: sign the freshly-linked binary with a
# STABLE identity, then exec it. Wired up in src-tauri/.cargo/config.toml.
#
# Why: `tauri dev` runs `cargo run`, which links an ad-hoc-signed binary whose
# code hash changes on every recompile. macOS pins a Keychain "Always Allow"
# decision to the binary's signature, so each rebuild reads as a new app and you
# get re-prompted for "Claude Code-credentials" endlessly. Signing every dev
# build with the same Apple Development cert gives a constant identity/Team ID,
# so the grant sticks. This script sits in the gap between cargo's link and run.

set -euo pipefail

BIN="$1"
shift

# Stable identity — keep in sync with macOS.signingIdentity in tauri.conf.json.
IDENTITY="${APPLE_SIGNING_IDENTITY:-Apple Development: calebcterry@outlook.com (2A4LGY86EP)}"
ENTITLEMENTS="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/Entitlements.plist"

# Only sign the actual app binary — skip test/bench harnesses (whose paths land
# under .../deps/ and carry a hash suffix), so `cargo test` stays fast and never
# touches the Keychain.
case "$BIN" in
  */deps/*) : ;;
  *)
    if [[ "$(uname)" == "Darwin" && -f "$BIN" ]]; then
      codesign --force --sign "$IDENTITY" \
        --entitlements "$ENTITLEMENTS" \
        --options runtime \
        "$BIN" >/dev/null 2>&1 \
        || echo "warn: codesign failed for $BIN (continuing unsigned)" >&2
    fi
    ;;
esac

exec "$BIN" "$@"
