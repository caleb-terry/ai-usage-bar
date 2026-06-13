# AI Usage Bar

A lightweight cross-platform **menu bar / system tray** monitor for your
**Claude Code** and **OpenAI Codex** usage limits. See your 5-hour and weekly
windows (or spend caps) at a glance, without opening a browser.

Built with [Tauri 2](https://v2.tauri.app/) — a small Rust core (~30–60 MB RAM)
with a minimal web UI. No Electron, no telemetry.

<p align="center">
  <img src="src-tauri/icons/providers/claude-color.png" width="48" alt="Claude" />
  &nbsp;&nbsp;&nbsp;
  <img src="src-tauri/icons/providers/codex-black.png" width="48" alt="Codex" />
</p>

---

## Features

- **Single switchable icon** for both providers. Pick one, or let **Auto** show
  whichever is closest to its limit.
- **Two display styles**: compact numbers (`46·10`) or dual progress bars,
  colored green / yellow / red by configurable thresholds.
- **Detail panel** on click (floating panel on Windows, popover on macOS) with
  reset countdowns, plan, and provider-specific extras (Codex credits, on-demand
  resets, code-review limits).
- **% used or % remaining**, your choice.
- **Spend-cap aware**: automatically switches to spend display when subscription
  session windows aren't available (enterprise / workspace accounts).
- **Launch at login**, configurable poll interval (1–15 min), light/dark aware.
- Native menu-bar agent app on macOS (no Dock icon).

---

## How it reads your usage

AI Usage Bar does **not** ask you to log in. It reuses the OAuth credentials the
official CLIs already store on your machine, **read-only**, and calls each
provider's own usage API directly. Nothing is sent anywhere else — no servers,
no analytics.

| Provider | Credential source | Usage endpoint |
| --- | --- | --- |
| **Claude Code** | macOS Keychain `Claude Code-credentials`, or `~/.claude/.credentials.json` | `api.anthropic.com/api/oauth/usage` |
| **Codex** | `~/.codex/auth.json` (also `$CODEX_HOME` / `~/.config/codex`), or keyring `Codex Auth` | `chatgpt.com/backend-api/wham/usage` |

Tokens are refreshed using each provider's standard OAuth refresh flow and
written back to the same store the CLI uses, so your terminal sessions stay
valid. Credential files are never bundled, logged, or transmitted anywhere
except the provider's own API.

> **macOS Keychain prompt:** the first time the app reads your Claude token, macOS
> shows a one-time Keychain authorization dialog. Click **Always Allow**. (Codex
> stores its token in a plain file, so it needs no prompt.)

---

## Prerequisites

You must already be signed in to the CLI(s) you want to monitor:

- **Claude Code** — install and run `claude`, sign in with a Pro / Max plan.
- **Codex** — install and run `codex`, sign in with your ChatGPT plan
  (Plus / Pro / Business). API-key-only setups are detected and shown as
  "API key mode — no subscription limits".

If a provider isn't signed in, its card shows **Sign-in needed** with the command
to run. You can disable either provider in Settings.

### Supported plan shapes

The app branches on what each API returns rather than hard-coding plan names, so
it works across:

- Subscription session windows (5-hour + 7-day) — the common case.
- Spend-cap / extra-usage accounts (Claude `extra_usage`, Codex
  `spend_control`) — shown as a single cap meter.
- API-key mode (Codex) — informational, no fake percentages.

---

## Install

Download the latest build from the [Releases](../../releases) page:

- **macOS**: `.dmg` (universal — Apple Silicon + Intel)
- **Windows**: `.msi` / `.exe`

### Unsigned builds

Early releases are unsigned. To open them:

- **macOS**: right-click the app → **Open**, then confirm. Or run
  `xattr -dr com.apple.quarantine "/Applications/AI Usage Bar.app"`.
- **Windows**: on the SmartScreen prompt, click **More info → Run anyway**.

---

## Build from source

Requirements: [Node 20+](https://nodejs.org), [pnpm 10+](https://pnpm.io),
[Rust stable](https://rustup.rs), and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install

# Run in dev (hot-reloads the UI; tray runs live)
pnpm tauri dev

# Produce installers for your platform
pnpm tauri build
```

### Verify the data layer against your real account

A manual smoke test fetches live usage using your local credentials and prints
the normalized result:

```bash
cd src-tauri
cargo run --example live_fetch          # both providers
cargo run --example live_fetch codex    # one provider
```

### Tests

```bash
cd src-tauri
cargo test            # provider parsing, normalization, selector, settings
```

---

## Settings

Right-click the tray icon or open **Settings…**:

| Setting | Default |
| --- | --- |
| Enabled providers | Both (disable either) |
| Active provider | Auto (highest utilization) |
| Tray style | Numbers |
| Show remaining % | Off (shows used %) |
| Poll interval | 3 minutes |
| Color thresholds | 50% (yellow) / 80% (red) |
| Launch at login | Off |
| Windows: float panel on click | On |

---

## Architecture

```
src-tauri/src/
├── providers/        # Provider trait + Claude & Codex (auth, client, normalize)
├── usage/            # Unified UsageSnapshot, aggregator, active-provider selector
├── tray/             # Icon renderer (numbers + bars), theme, context menu
├── windows/          # Windows float panel / macOS popover
├── settings.rs       # Persisted JSON prefs
├── commands.rs       # Tauri IPC
└── lib.rs            # Wiring + background poll loop
src/                  # React + TypeScript settings & detail UI
```

Every provider normalizes its raw API response into one `UsageSnapshot` shape, so
the tray, settings, and detail panel never branch on provider specifics.

---

## Privacy & security

- Local credential access is **read-only**; tokens are only ever sent to the
  providers' own endpoints (`api.anthropic.com`, `platform.claude.com`,
  `chatgpt.com`, `auth.openai.com`).
- No telemetry, analytics, or third-party network calls.
- Token refreshes are conservative and written back atomically to avoid
  disrupting concurrent CLI sessions.

---

## Acknowledgements

API behavior was learned from the excellent reverse-engineering docs at
[openusage](https://github.com/robinebers/openusage) and
[CodexBar](https://github.com/steipete/CodexBar). This project studies but does
not fork them.

Provider logos are property of their respective owners (Anthropic, OpenAI) and
are used here for identification only.

## License

[CC BY-NC-SA 4.0](LICENSE) — free for non-commercial use with attribution;
derivatives share alike. Commercial use requires separate permission.
