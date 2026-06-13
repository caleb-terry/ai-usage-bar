// Dev-only fake Tauri bridge so the web UI is fully previewable in a plain
// browser (Claude Preview, `pnpm dev`) without the Rust backend.
//
// It installs `window.__TAURI_INTERNALS__` implementing the four app commands
// (`get_usage`, `get_settings`, `set_settings`, `refresh_now`) plus the
// `plugin:event|*` plumbing that `@tauri-apps/api`'s `listen`/`emit` ride on.
// The app talks to this exactly as it would the real bridge — no per-component
// fallbacks. Imported behind `import.meta.env.DEV` in main.tsx, so it never
// ships in the Tauri/production bundle.

import type {
  CostSummary,
  Incident,
  Settings,
  UsageReport,
  UsageSnapshot,
} from "../api";

const hoursFromNow = (h: number) =>
  new Date(Date.now() + h * 3600 * 1000).toISOString();

let settings: Settings = {
  enabled_providers: ["claude", "codex", "openrouter", "elevenlabs"],
  active_provider: "auto",
  display_style: "bars",
  show_remaining: true,
  poll_interval_secs: 60,
  thresholds: { warn: 70, danger: 90 },
  windows_float_panel: true,
  launch_at_login: false,
  language: "system",
  default_terminal: "terminal",
  show_cost_summary: true,
  cost_history_days: 30,
  check_provider_status: true,
  session_quota_notifications: true,
  quota_warning_notifications: true,
};

// Vary the numbers slightly on each refresh so Refresh visibly does something.
function snapshots(jitter = 0): Record<string, UsageSnapshot> {
  const codexPrimary = Math.min(100, 12 + jitter * 7);
  const codexWeek = Math.min(100, 9 + jitter * 3);
  return {
    codex: {
      provider: "codex",
      plan_label: "Prolite",
      stale: false,
      fetched_at: hoursFromNow(0),
      mode: {
        kind: "session",
        primary: { utilization: codexPrimary, reset_at: hoursFromNow(4.98), label: "5h" },
        secondary: { utilization: codexWeek, reset_at: hoursFromNow(154), label: "Week" },
      },
      extras: { credit_balance_cents: 0, on_demand_resets: 1 },
    },
    claude: {
      provider: "claude",
      plan_label: "Max",
      stale: jitter === 0,
      fetched_at: hoursFromNow(0),
      mode: {
        kind: "session",
        primary: { utilization: Math.min(100, 31 + jitter * 9), reset_at: hoursFromNow(5), label: "5h" },
        secondary: { utilization: Math.min(100, 18 + jitter * 4), reset_at: hoursFromNow(160), label: "Week" },
      },
      extras: {
        code_review_utilization: 22,
        extra_usage_used_cents: 58,
        extra_usage_cap_cents: 5000,
      },
    },
    openrouter: {
      provider: "openrouter",
      plan_label: "credits",
      stale: false,
      fetched_at: hoursFromNow(0),
      mode: { kind: "api_key_only" },
      extras: { credit_balance_cents: 1850 + jitter * 5 },
    },
    elevenlabs: {
      provider: "elevenlabs",
      plan_label: "creator",
      stale: false,
      fetched_at: hoursFromNow(0),
      mode: {
        kind: "session",
        primary: {
          utilization: Math.min(100, 42 + jitter * 6),
          reset_at: hoursFromNow(240),
          label: "Chars",
        },
      },
      extras: {},
    },
  };
}

// Mock API-key store: which credit providers have a key. set_api_key mutates it.
const apiKeys: Record<string, boolean> = {
  openrouter: true,
  elevenlabs: true,
  groq: false,
  deepgram: false,
  zai: false,
  minimax: false,
  gemini: false,
  grok: false,
  deepseek: false,
  moonshot: false,
  mistral: false,
  perplexity: false,
};

// Mock incidents: only when status checking is on. Shows one minor Codex
// incident so the preview can exercise the banner; cleared on later refreshes
// to also exercise the resolved path.
function incidents(): Incident[] {
  if (!settings.check_provider_status) return [];
  if (refreshTick % 2 === 1) return [];
  return [{ provider: "codex", severity: "minor", description: "Degraded Performance" }];
}

let refreshTick = 0;
function report(): UsageReport {
  return {
    snapshots: snapshots(refreshTick),
    active: "codex",
    settings,
    incidents: incidents(),
  };
}

// Mock local cost summary, scaled lightly by the refresh tick so Refresh moves
// the numbers. Returns null when the user disabled the cost summary, matching
// the real `get_cost_summary` command.
function costSummary(): CostSummary | null {
  if (!settings.show_cost_summary) return null;
  const bump = 1 + refreshTick * 0.04;
  const claudeToday = 144.19 * bump;
  const codexToday = 6.4 * bump;
  return {
    window_days: settings.cost_history_days,
    total_today_usd: claudeToday + codexToday,
    total_window_usd: 6755.39 + 537.52,
    providers: {
      claude: {
        today_usd: claudeToday,
        today_tokens: 58_533_609,
        window_usd: 6755.39,
        window_tokens: 3_053_017_278,
      },
      codex: {
        today_usd: codexToday,
        today_tokens: 4_120_500,
        window_usd: 537.52,
        window_tokens: 731_618_856,
      },
    },
  };
}

// --- event plumbing -------------------------------------------------------
// `transformCallback` hands the event plugin an id that maps to a stored fn;
// `plugin:event|listen` records which event that id is for; `emit` dispatches.
type Cb = (payload: unknown) => void;
const callbacks = new Map<number, Cb>();
const listeners = new Map<string, Set<number>>(); // event name -> callback ids
let nextId = 1;
let nextEventId = 1;

function dispatch(event: string, payload: unknown = null) {
  const ids = listeners.get(event);
  if (!ids) return;
  for (const id of ids) {
    callbacks.get(id)?.({ event, id, payload });
  }
}

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "get_usage":
    case "refresh_now":
      if (cmd === "refresh_now") refreshTick += 1;
      return report();
    case "get_cost_summary":
      return costSummary();
    case "api_key_status":
      return { ...apiKeys };
    case "set_api_key": {
      const provider = args.provider as string;
      const key = (args.key as string) ?? "";
      apiKeys[provider] = key.trim().length > 0;
      queueMicrotask(() => dispatch("usage-updated", null));
      return null;
    }
    case "get_settings":
      return settings;
    case "set_settings":
      settings = (args.settings as Settings) ?? settings;
      // Mirror the real backend: a settings change re-emits usage-updated.
      queueMicrotask(() => dispatch("usage-updated", null));
      return null;
    case "open_terminal":
      // No real process to spawn in the browser; log so previews can confirm.
      console.log(`[mock] open_terminal → ${settings.default_terminal}`);
      return null;

    // Event plugin commands used by @tauri-apps/api's listen()/emit().
    case "plugin:event|listen": {
      const event = args.event as string;
      const handlerId = args.handler as number;
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event)!.add(handlerId);
      return nextEventId++;
    }
    case "plugin:event|unlisten": {
      const event = args.event as string;
      const eventId = args.eventId as number;
      // We keyed by handler id, not eventId; clear the whole set on unlisten —
      // fine for the preview's single listener per event.
      listeners.get(event)?.delete(eventId);
      return null;
    }
    case "plugin:event|emit":
    case "plugin:event|emit_to":
      dispatch(args.event as string, args.payload);
      return null;

    default:
      // WebviewWindow.show()/setFocus() and friends — harmless no-ops.
      return null;
  }
}

export function installMockTauri() {
  const w = window as unknown as Record<string, unknown>;
  if (w.__TAURI_INTERNALS__) return; // real bridge present — do nothing

  w.isTauri = true;
  w.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback(cb: Cb, _once = false) {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc(path: string) {
      return path;
    },
    metadata: {
      currentWindow: { label: "panel" },
      currentWebview: { label: "panel" },
    },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event: string, eventId: number) {
      listeners.get(event)?.delete(eventId);
    },
  };

  // Emulate the backend's poll loop: nudge the UI every few seconds so the
  // live-update path is exercised in preview.
  setInterval(() => dispatch("usage-updated", null), 8000);
}
