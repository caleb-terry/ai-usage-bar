// Typed wrappers around the Tauri IPC commands. The shapes mirror the Rust
// structs in src-tauri/src/{usage/types.rs, settings.rs, commands.rs}.

import { invoke } from "@tauri-apps/api/core";

/// Single source of truth for provider metadata, mirroring Rust's
/// `PROVIDER_META` in `src-tauri/src/usage/types.rs`. Everything below
/// (`ProviderId`, the label/accent maps, `API_KEY_PROVIDERS`) is *derived* from
/// this one table so adding a provider is a single row edit on each side. The
/// Rust test `provider_metadata_matches_typescript` guards against drift.
///
/// `kind: "api_key"` marks the non-subscription providers (everything except
/// the two CLI subscription providers, Claude and Codex).
interface ProviderMeta {
  id: string;
  label: string;
  tabLabel: string;
  accent: string;
  kind: "subscription" | "api_key";
}

const PROVIDER_META = [
  { id: "claude", label: "Claude Code", tabLabel: "Claude", accent: "#d97757", kind: "subscription" },
  { id: "codex", label: "Codex", tabLabel: "Codex", accent: "#10a37f", kind: "subscription" },
  { id: "openrouter", label: "OpenRouter", tabLabel: "OpenRouter", accent: "#6566f1", kind: "api_key" },
  { id: "elevenlabs", label: "ElevenLabs", tabLabel: "11Labs", accent: "#000000", kind: "api_key" },
  { id: "groq", label: "Groq", tabLabel: "Groq", accent: "#f55036", kind: "api_key" },
  { id: "deepgram", label: "Deepgram", tabLabel: "Deepgram", accent: "#13ef93", kind: "api_key" },
  { id: "zai", label: "z.ai", tabLabel: "z.ai", accent: "#3b82f6", kind: "api_key" },
  { id: "minimax", label: "MiniMax", tabLabel: "MiniMax", accent: "#ff4f4f", kind: "api_key" },
  { id: "gemini", label: "Gemini", tabLabel: "Gemini", accent: "#4285f4", kind: "api_key" },
  { id: "grok", label: "Grok", tabLabel: "Grok", accent: "#1a1a1a", kind: "api_key" },
  { id: "deepseek", label: "DeepSeek", tabLabel: "DeepSeek", accent: "#4d6bfe", kind: "api_key" },
  { id: "moonshot", label: "Moonshot", tabLabel: "Moonshot", accent: "#16191e", kind: "api_key" },
  { id: "mistral", label: "Mistral", tabLabel: "Mistral", accent: "#fa520f", kind: "api_key" },
  { id: "perplexity", label: "Perplexity", tabLabel: "Perplexity", accent: "#20808d", kind: "api_key" },
] as const satisfies readonly ProviderMeta[];

export type ProviderId = (typeof PROVIDER_META)[number]["id"];
export type ActiveProvider = "auto" | "claude" | "codex";

/// API-key providers (everything except the two CLI subscription providers),
/// derived from the metadata table.
export const API_KEY_PROVIDERS: ProviderId[] = PROVIDER_META.filter(
  (m) => m.kind === "api_key",
).map((m) => m.id);

/// The CLI subscription providers (Claude, Codex), derived from the table so a
/// future subscription provider appears in the Connections section automatically.
export const SUBSCRIPTION_PROVIDERS: ProviderId[] = PROVIDER_META.filter(
  (m) => m.kind === "subscription",
).map((m) => m.id);

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
  | { kind: "credit_balance"; balance_cents: number }
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

export type Severity = "none" | "minor" | "major" | "critical";

export interface Incident {
  provider: ProviderId;
  severity: Severity;
  description: string;
}

export interface UsageReport {
  snapshots: Partial<Record<ProviderId, UsageSnapshot>>;
  active?: ProviderId;
  settings: Settings;
  incidents: Incident[];
}

export interface ProviderCost {
  today_usd: number;
  today_tokens: number;
  window_usd: number;
  window_tokens: number;
}

export interface CostSummary {
  providers: Partial<Record<ProviderId, ProviderCost>>;
  total_today_usd: number;
  total_window_usd: number;
  window_days: number;
}

export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) =>
  invoke<void>("set_settings", { settings });
export const getUsage = () => invoke<UsageReport>("get_usage");
export const refreshNow = () => invoke<UsageReport>("refresh_now");
export const openTerminal = () => invoke<void>("open_terminal");
/// Local cost summary scanned from session logs. Returns null when the user
/// has the cost summary disabled.
export const getCostSummary = () =>
  invoke<CostSummary | null>("get_cost_summary");

/// Store or clear (empty string clears) an API key for a credit provider.
export const setApiKey = (provider: ProviderId, key: string) =>
  invoke<void>("set_api_key", { provider, key });

/// Which API-key providers currently have a key stored (never returns secrets).
export const apiKeyStatus = () =>
  invoke<Partial<Record<ProviderId, boolean>>>("api_key_status");

/// Compact token count: 58_533_609 → "58.5M", 731_618 → "732K".
export function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${Math.round(n / 1_000)}K`;
  return String(n);
}

/// Format a USD figure: "$0.00", "$12.34", "$7,293".
export function formatUsd(n: number): string {
  if (n >= 1000) return `$${Math.round(n).toLocaleString("en-US")}`;
  return `$${n.toFixed(2)}`;
}

/// Build a `Record<ProviderId, T>` by projecting one field out of the metadata
/// table, so the label/accent maps can never list a different provider set than
/// `PROVIDER_META`.
function metaMap<T>(pick: (m: ProviderMeta) => T): Record<ProviderId, T> {
  return Object.fromEntries(
    PROVIDER_META.map((m) => [m.id, pick(m)]),
  ) as Record<ProviderId, T>;
}

/// Human-facing display names, derived from the metadata table.
export const PROVIDER_LABEL: Record<ProviderId, string> = metaMap((m) => m.label);

/// Short label used on the compact tab chips, derived from the metadata table.
export const PROVIDER_TAB_LABEL: Record<ProviderId, string> = metaMap(
  (m) => m.tabLabel,
);

/// Per-provider brand accent. Drives the tab underline and the hero card's
/// fill, so the panel reads at a glance which provider you're looking at.
export const PROVIDER_ACCENT: Record<ProviderId, string> = metaMap(
  (m) => m.accent,
);

/// Color for a service-status severity badge.
export function severityColor(s: Severity): string {
  switch (s) {
    case "critical":
    case "major":
      return "var(--danger)";
    case "minor":
      return "var(--warn)";
    default:
      return "var(--ok)";
  }
}

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
