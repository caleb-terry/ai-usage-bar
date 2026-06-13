// Typed wrappers around the Tauri IPC commands. The shapes mirror the Rust
// structs in src-tauri/src/{usage/types.rs, settings.rs, commands.rs}.

import { invoke } from "@tauri-apps/api/core";

export type ProviderId = "claude" | "codex";
export type ActiveProvider = "auto" | "claude" | "codex";
export type DisplayStyle = "numbers" | "bars";
export type Language = "system" | "en";
export type TerminalApp = "terminal" | "iterm" | "warp" | "ghostty";

export interface WindowUsage {
  utilization: number;
  reset_at?: string;
  label: string;
}

export type DisplayMode =
  | { kind: "session"; primary: WindowUsage; secondary?: WindowUsage }
  | {
      kind: "spend_cap";
      utilization: number;
      used_cents?: number;
      limit_cents?: number;
      reset_at?: string;
    }
  | { kind: "unauthenticated" }
  | { kind: "api_key_only" };

export interface DetailExtras {
  credit_balance_cents?: number;
  code_review_utilization?: number;
  on_demand_resets?: number;
  extra_usage_used_cents?: number;
  extra_usage_cap_cents?: number;
}

export interface UsageSnapshot {
  provider: ProviderId;
  plan_label: string;
  mode: DisplayMode;
  fetched_at: string;
  stale: boolean;
  extras: DetailExtras;
}

export interface Thresholds {
  warn: number;
  danger: number;
}

export interface Settings {
  enabled_providers: ProviderId[];
  active_provider: ActiveProvider;
  display_style: DisplayStyle;
  show_remaining: boolean;
  poll_interval_secs: number;
  thresholds: Thresholds;
  windows_float_panel: boolean;
  launch_at_login: boolean;
  language: Language;
  default_terminal: TerminalApp;
  show_cost_summary: boolean;
  cost_history_days: number;
  check_provider_status: boolean;
  session_quota_notifications: boolean;
  quota_warning_notifications: boolean;
}

export interface UsageReport {
  snapshots: Partial<Record<ProviderId, UsageSnapshot>>;
  active?: ProviderId;
  settings: Settings;
}

export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) =>
  invoke<void>("set_settings", { settings });
export const getUsage = () => invoke<UsageReport>("get_usage");
export const refreshNow = () => invoke<UsageReport>("refresh_now");
export const openTerminal = () => invoke<void>("open_terminal");

export const PROVIDER_LABEL: Record<ProviderId, string> = {
  claude: "Claude Code",
  codex: "Codex",
};

/// Short label used on the compact tab chips.
export const PROVIDER_TAB_LABEL: Record<ProviderId, string> = {
  claude: "Claude",
  codex: "Codex",
};

/// Per-provider brand accent. Drives the tab underline and the hero card's
/// fill, so the panel reads at a glance which provider you're looking at.
export const PROVIDER_ACCENT: Record<ProviderId, string> = {
  claude: "#d97757",
  codex: "#10a37f",
};

/// Pick a threshold color for a utilization percentage.
export function thresholdColor(util: number, t: Thresholds): string {
  if (util >= t.danger) return "var(--danger)";
  if (util >= t.warn) return "var(--warn)";
  return "var(--ok)";
}

/// Apply the show-remaining preference.
export function displayPct(util: number, showRemaining: boolean): number {
  return showRemaining ? 100 - util : util;
}

/// Format a reset timestamp as a relative "in 3h 12m".
export function formatReset(iso?: string): string | null {
  if (!iso) return null;
  const ms = new Date(iso).getTime() - Date.now();
  if (Number.isNaN(ms)) return null;
  if (ms <= 0) return "now";
  const mins = Math.floor(ms / 60000);
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h >= 24) {
    const d = Math.floor(h / 24);
    return `${d}d ${h % 24}h`;
  }
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

/// Format a past timestamp as a relative "7m ago" / "2h ago" / "3d ago".
/// Returns null for missing/invalid input so callers can omit the line.
export function formatRelative(iso?: string): string | null {
  if (!iso) return null;
  const ms = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(ms)) return null;
  if (ms < 60000) return "just now";
  const mins = Math.floor(ms / 60000);
  if (mins < 60) return `${mins}m ago`;
  const h = Math.floor(mins / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}
