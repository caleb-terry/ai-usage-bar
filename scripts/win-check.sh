#!/usr/bin/env bash
#
# Cross-compile-check the Rust/Tauri backend for Windows from any non-Windows
# host (this repo's dev machine is macOS arm64, which can't build the Windows
# target natively and has no /dev/kvm for the GUI test VM).
#
# Why this exists: CI (.github/workflows/ci.yml) only runs on macOS, so it
# NEVER compiles the Windows target. Code behind `#[cfg(target_os = "windows")]`
# / `#[cfg(windows)]` (e.g. the tray left-click branch in lib.rs, the
# windows-registry + keyring windows-native deps) is otherwise completely
# unchecked until a tagged release build runs release.yml. This catches those
# errors locally in a few minutes.
#
# What it does: spins up a throwaway Linux `rust` container, adds the
# `x86_64-pc-windows-gnu` target + the mingw-w64 linker, and runs
# `cargo check --target x86_64-pc-windows-gnu`. We `check` rather than `build`
# on purpose: it type-checks and runs codegen-readiness for the Windows target
# (which is what surfaces cfg-gated compile errors) without the slow, fragile
# final GUI link that the real MSVC release build does.
#
# Fidelity caveat: this uses the GNU toolchain (`-gnu`), while the actual
# release (release.yml) uses `x86_64-pc-windows-msvc`. The two share the same
# Rust front-end, so this catches the vast majority of cfg-gated errors, but it
# is a strong PROXY, not a byte-identical reproduction of the release toolchain.
# The authoritative MSVC build still happens in release.yml.
#
# This is the COMPILE check. It is unrelated to docker-compose.windows.yml,
# which boots a full Windows 11 GUI VM to smoke-test the built .msi/.exe
# installer (and which needs /dev/kvm — a Linux host).
#
# Usage:
#   scripts/win-check.sh            # cargo check the Windows target
#   WIN_CHECK_CLIPPY=1 scripts/win-check.sh   # also run clippy -D warnings
#
# Named Docker volumes cache the cargo registry and the Windows target dir, so
# the first run is slow (image pull + dep compile) and subsequent runs are fast.

set -euo pipefail

# Resolve the repo root from this script's location so it works from any cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

IMAGE="rust:latest"
TARGET="x86_64-pc-windows-gnu"
REGISTRY_VOL="aub-cargo-registry"   # cargo registry cache (shared, reusable)
TARGET_VOL="aub-win-target"         # Windows target/ dir (keeps incremental state)

# Optionally also run clippy with -D warnings against the Windows target.
RUN_CLIPPY="${WIN_CHECK_CLIPPY:-0}"

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found on PATH" >&2
  exit 1
fi

echo "==> Windows cross-compile check ($TARGET) via $IMAGE"
echo "    repo: $REPO_ROOT"
[ "$RUN_CLIPPY" = "1" ] && echo "    clippy: enabled (-D warnings)"

# The in-container build steps. Kept as a single heredoc so the whole thing is
# one `docker run` and easy to read. Errors propagate (set -e) -> non-zero exit.
read -r -d '' INNER <<'INNER_SCRIPT' || true
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

echo "--> installing mingw-w64 linker"
apt-get update -qq >/dev/null
apt-get install -y -qq gcc-mingw-w64-x86-64 >/dev/null

echo "--> adding rust target ${TARGET}"
rustup target add "${TARGET}" >/dev/null
[ "${RUN_CLIPPY}" = "1" ] && rustup component add clippy >/dev/null 2>&1 || true

cd /work/src-tauri

echo "--> cargo check --target ${TARGET} --bins"
cargo check --target "${TARGET}" --bins

if [ "${RUN_CLIPPY}" = "1" ]; then
  echo "--> cargo clippy --target ${TARGET} -- -D warnings"
  cargo clippy --target "${TARGET}" --bins -- -D warnings
fi

echo "--> OK: Windows target compiles"
INNER_SCRIPT

docker run --rm \
  -e TARGET="$TARGET" \
  -e RUN_CLIPPY="$RUN_CLIPPY" \
  -v "$REPO_ROOT":/work \
  -v "$REGISTRY_VOL":/usr/local/cargo/registry \
  -v "$TARGET_VOL":/work/src-tauri/target \
  -w /work \
  "$IMAGE" \
  bash -c "$INNER"
