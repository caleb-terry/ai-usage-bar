# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A Tauri 2 menu-bar / system-tray app monitoring Claude Code and OpenAI Codex usage limits. Rust core + React/TypeScript UI. Reads the official CLIs' stored OAuth credentials **read-only** and calls each provider's own usage API — no servers, no telemetry.

## Commands

```bash
pnpm install
pnpm tauri dev            # run the app (UI hot-reloads, tray runs live)
pnpm tauri build          # platform installers
pnpm dev                  # vite-only (browser preview via the mock bridge; no tray)

cd src-tauri
cargo test                # all Rust tests
cargo test settings::     # one module
cargo test roundtrips_through_json   # one test by name
cargo run --example live_fetch [claude|codex]   # live fetch w/ your real creds, prints normalized snapshot
```

`cargo` may not be on `PATH` in some shells — use `~/.cargo/bin/cargo`. Frontend typecheck: `npx tsc --noEmit` (strict, `noUnusedLocals` on — unused imports are hard errors).

## Architecture

**The unifying contract is `UsageSnapshot`** (`src-tauri/src/usage/types.rs`). Every provider implements the `Provider` trait (`providers/mod.rs`: `id` / `has_credentials` / `async fetch`) and normalizes its raw API response into one `UsageSnapshot` + `DisplayMode` enum (`Session` / `SpendCap` / `Unauthenticated` / `ApiKeyOnly`). The tray, settings, and panel **never branch on provider specifics** — they read the snapshot. Adding a provider = new module under `providers/` + a `ProviderId` variant; downstream code is untouched.

**Data flow:** `lib.rs::poll_loop` (background task) → `Aggregator::poll_enabled` fetches enabled providers and returns only those whose *displayed* snapshot changed (`snapshots_visually_equal` ignores timestamps, so identical data doesn't repaint) → `selector::resolve_active` picks the active provider (explicit or `Auto` = highest `peak_utilization`) → `update_tray` re-renders icon/title/menu. The loop also fires edge-triggered quota notifications via `notify::NotifyState` and emits `usage-updated` to open windows.

**Two webviews, one bundle** (`tauri.conf.json` windows `settings` + `panel`, both `index.html?view=…`). `App.tsx` reads `?view` and renders `Settings` or `UsageDetail`. macOS gets a real `NSVisualEffectView` behind the transparent panel (vibrancy); everywhere else paints a solid fill (`.no-vibrancy`). macOS runs as an accessory app (no Dock icon).

**Settings** (`settings.rs`) persist as JSON in the platform config dir, struct-level `#[serde(default)]` so old files load after new fields are added. Changes flow through `mutate_settings` (or the `set_settings` IPC command), which persists, refreshes the tray, and re-emits `usage-updated`. The tray context menu (`tray/menu.rs`) and the settings UI are two front-ends to the same settings.

**Frontend ↔ backend:** typed IPC wrappers in `src/api.ts` mirror the Rust structs. The UI is backend-agnostic — in a plain browser, `src/dev/mockTauri.ts` installs a fake `__TAURI_INTERNALS__` implementing the commands + event plumbing, so the full UI is previewable without Rust. Keep the mock's `Settings` literal in sync when adding fields, or `tsc` fails.

## Critical gotchas

- **Never read the macOS Keychain on the startup/`setup` path.** It blocks on the SecurityAgent permission dialog and can deadlock app launch. Credential reads (Claude token) must stay inside the async `fetch` path / `spawn_blocking`. `lib.rs` startup deliberately loads settings off disk without touching the credential store.
- **Dev re-prompt loop:** `src-tauri/.cargo/config.toml` wires a `runner` (`scripts/sign-and-run.sh`) that re-signs each dev build with a *stable* Apple Development identity. Without it, every recompile changes the binary's code hash, macOS treats it as a new app, and you get re-prompted for Keychain access endlessly. Don't remove it.
- **i18n** (`src/i18n/`) ships English only; `strings(language)` resolves the active locale, falling back to `navigator.language` when set to `System`. Add a locale = add a `Strings` record + a case; the structure is the extension point.

## Code style

Both the Rust and TS sides comment the *why* behind non-obvious logic densely (see `lib.rs`, `panel.css`, `mockTauri.ts`). Match that density on new code — explain intent and gotchas, not mechanics.
