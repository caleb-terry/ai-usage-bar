---
name: windows-compile-check
description: Cross-compile-check the Rust/Tauri backend for Windows from this macOS host. Use whenever Windows-specific code changes — anything behind `#[cfg(target_os = "windows")]` / `#[cfg(windows)]`, the `windows-registry` or `keyring` windows-native paths, the tray left-click branch in lib.rs — or before merging a branch that touches them, since CI does NOT compile the Windows target. Trigger on requests like "check the windows build", "does this compile on windows", "verify the cfg(windows) branch".
---

# Windows compile check

## Why this exists

CI ([.github/workflows/ci.yml](../../../.github/workflows/ci.yml)) runs **only on `macos-latest`** — it never compiles the Windows target. Code behind `#[cfg(target_os = "windows")]` / `#[cfg(windows)]` (the tray left-click branch in [src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs), the `windows-registry` dependency, the `keyring` `windows-native` feature) is therefore **unchecked anywhere** until a tagged release runs `release.yml`. This skill catches those errors locally in a few minutes.

This is the **compile** check. It is unrelated to [docker-compose.windows.yml](../../../docker-compose.windows.yml), which boots a full Windows 11 GUI VM to smoke-test the built `.msi`/`.exe` installer (and needs `/dev/kvm`, i.e. a Linux host). Don't conflate them: compile errors → this skill; "does the installed app's tray icon appear" → the VM.

## How to run

```bash
scripts/win-check.sh                      # cargo check the Windows target
WIN_CHECK_CLIPPY=1 scripts/win-check.sh   # also clippy -D warnings
```

It spins up a throwaway `rust` Linux container, adds the `x86_64-pc-windows-gnu`
target + the mingw-w64 linker, and runs `cargo check --target x86_64-pc-windows-gnu --bins`.
Named Docker volumes (`aub-cargo-registry`, `aub-win-target`) cache the registry
and target dir, so the first run is slow (image pull + full dep compile, ~5-10 min)
and later runs are fast (incremental).

**Success looks like:** the script prints `--> OK: Windows target compiles` and
exits 0. A non-zero exit with `error[...]` lines means the Windows target does
not compile — read the error, fix the source, re-run.

## Reading the output

- **Errors (`error[E...]` / `error:`)** → real problems. Fix them.
- **`dead_code` / `unused` warnings** are EXPECTED and not failures. When
  compiling *for Windows*, the macOS-only code (e.g. `render_provider_glyph`,
  `GLYPH_SIZE` in [src-tauri/src/tray/icon.rs](../../../src-tauri/src/tray/icon.rs))
  is cfg'd out, so it shows as unused. That's the cross-target view working as
  intended — don't "fix" it by deleting macOS code. Only treat warnings as
  blocking if you ran with `WIN_CHECK_CLIPPY=1` (which adds `-D warnings`), and
  even then scope any change to genuinely-Windows code.

## Fidelity caveat — important

This uses the **GNU** toolchain (`x86_64-pc-windows-gnu`), while the real release
([release.yml](../../../.github/workflows/release.yml)) uses
`x86_64-pc-windows-msvc`. They share the same Rust front-end, so this catches the
vast majority of cfg-gated compile errors — but it is a strong **proxy**, not a
byte-identical reproduction. The authoritative MSVC build is still `release.yml`.
If a change is risky and MSVC-specific (linker flags, `windows`/`windows-sys`
ABI edge cases), note that this check passing is necessary-but-not-sufficient and
the release build is the final word.

## Requirements

- Docker running on the host (this Mac has Docker but no `/dev/kvm`; this skill
  needs neither KVM nor a Windows host — it's a plain Linux container).
