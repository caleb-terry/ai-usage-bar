// The five settings tabs. Each is a thin layout over the shared controls; all
// state lives in the parent Settings component, which passes `s` (current
// settings) and `update` (optimistic patch + persist).

import { useEffect, useState } from "react";
import {
  ActiveProvider,
  API_KEY_PROVIDERS,
  apiKeyStatus,
  DisplayStyle,
  Language,
  PROVIDER_LABEL,
  ProviderId,
  resetSettings,
  Settings as SettingsType,
  SUBSCRIPTION_PROVIDERS,
  TerminalApp,
  UsageReport,
  formatRelative,
} from "../../api";
import { Strings } from "../../i18n";
import { cadenceOptions, isWindows, Row, Section, Select, Stepper, Toggle } from "./controls";
import { GridIcon } from "./icons";
import { ApiKeyProviderRow } from "./ApiKeyProviderRow";

const APP_VERSION = "0.1.0";

/// Per-request HTTP timeout (matches the reqwest client built in lib.rs). Shown
/// in the General tab's auto-refresh readout so the value is honest rather than
/// a hardcoded placeholder.
const REQUEST_TIMEOUT_LABEL = "20s";

export interface TabProps {
  t: Strings;
  s: SettingsType;
  // Async (optimistic patch + persist); typed as a promise so a future
  // post-update step in a tab can await it rather than silently racing persist.
  update: (patch: Partial<SettingsType>) => Promise<void>;
}

export function GeneralTab({
  t,
  s,
  report,
  update,
}: TabProps & { report: UsageReport }) {
  const cadences = cadenceOptions(t);
  const cadenceLabel = cadences.find((o) => o.secs === s.poll_interval_secs);
  return (
    <>
      <Section title={t.sectionSystem}>
        <Row title={t.language} desc={t.languageDesc}>
          <Select
            value={s.language}
            onChange={(v) => {
              update({ language: v as Language });
              // The string set resolves at render; a reload mirrors the native
              // "requires restart" semantics for anything cached at mount.
              setTimeout(() => window.location.reload(), 50);
            }}
            options={[
              { value: "system", label: t.languageSystem },
              { value: "en", label: t.languageEnglish },
            ]}
          />
        </Row>
        <Row title={t.defaultTerminal} desc={t.defaultTerminalDesc}>
          <Select
            value={s.default_terminal}
            onChange={(v) => update({ default_terminal: v as TerminalApp })}
            options={[
              { value: "terminal", label: "Terminal" },
              { value: "iterm", label: "iTerm" },
              { value: "warp", label: "Warp" },
              { value: "ghostty", label: "Ghostty" },
            ]}
          />
        </Row>
        <Toggle
          title={t.startAtLogin}
          desc={t.startAtLoginDesc}
          checked={s.launch_at_login}
          onChange={(v) => update({ launch_at_login: v })}
        />
      </Section>

      <Section title={t.sectionUsage}>
        <Toggle
          title={t.showCostSummary}
          desc={t.showCostSummaryDesc}
          checked={s.show_cost_summary}
          onChange={(v) => update({ show_cost_summary: v })}
        />
        {s.show_cost_summary && (
          <>
            <Row
              title={t.historyWindow}
              value={t.historyWindowValue(s.cost_history_days)}
            >
              <Stepper
                value={s.cost_history_days}
                min={1}
                max={90}
                step={s.cost_history_days < 7 ? 1 : 7}
                onChange={(v) => update({ cost_history_days: v })}
              />
            </Row>
            <div className="settings-readout">
              <div>
                {t.autoRefreshLine(
                  cadenceLabel?.label ?? `${s.poll_interval_secs}s`,
                  REQUEST_TIMEOUT_LABEL,
                )}
              </div>
              {SUBSCRIPTION_PROVIDERS.map((id) => {
                const snap = report.snapshots[id];
                if (!snap) return null;
                const ago = formatRelative(snap.fetched_at) ?? t.never;
                return (
                  <div key={id}>
                    {t.providerUpdatedLine(
                      PROVIDER_LABEL[id],
                      ago,
                      s.cost_history_days,
                    )}
                  </div>
                );
              })}
            </div>
          </>
        )}
      </Section>

      <Section title={t.sectionAutomation}>
        <Row title={t.refreshCadence} desc={t.refreshCadenceDesc}>
          <Select
            value={String(s.poll_interval_secs)}
            onChange={(v) => update({ poll_interval_secs: Number(v) })}
            options={cadences.map((o) => ({
              value: String(o.secs),
              label: o.label,
            }))}
          />
        </Row>
        <Toggle
          title={t.checkProviderStatus}
          desc={t.checkProviderStatusDesc}
          checked={s.check_provider_status}
          onChange={(v) => update({ check_provider_status: v })}
        />
        <Toggle
          title={t.sessionQuotaNotifications}
          desc={t.sessionQuotaNotificationsDesc}
          checked={s.session_quota_notifications}
          onChange={(v) => update({ session_quota_notifications: v })}
        />
        <Toggle
          title={t.quotaWarningNotifications}
          desc={t.quotaWarningNotificationsDesc}
          checked={s.quota_warning_notifications}
          onChange={(v) => update({ quota_warning_notifications: v })}
        />
      </Section>
    </>
  );
}

export function ProvidersTab({
  t,
  s,
  report,
  update,
}: TabProps & { report: UsageReport }) {
  // Which API-key providers currently have a key stored. Fetched once here and
  // passed down, rather than each row firing its own apiKeyStatus() IPC call
  // (which re-reads + parses the whole config file per row).
  const [keyStatus, setKeyStatus] = useState<Partial<Record<ProviderId, boolean>>>(
    {},
  );

  const refreshKeys = async () => {
    try {
      setKeyStatus(await apiKeyStatus());
    } catch {
      /* ignore — rows fall back to "no key" */
    }
  };

  useEffect(() => {
    refreshKeys();
  }, []);

  const toggleProvider = (id: ProviderId) => {
    const enabled = s.enabled_providers.includes(id)
      ? s.enabled_providers.filter((p) => p !== id)
      : [...s.enabled_providers, id];
    update({ enabled_providers: enabled });
  };

  const statusFor = (
    id: ProviderId,
  ): { text: string; tone: "ok" | "warn" | "dim" } => {
    const snap = report.snapshots[id];
    if (!snap) return { text: t.statusNotEnabled, tone: "dim" };
    switch (snap.mode.kind) {
      case "unauthenticated":
        return { text: t.statusSignIn, tone: "warn" };
      case "api_key_only":
        return { text: t.statusApiKey, tone: "warn" };
      default:
        return snap.stale
          ? { text: t.statusConnectedStale, tone: "warn" }
          : { text: t.statusConnected, tone: "ok" };
    }
  };

  return (
    <>
      <Section title={t.sectionConnections}>
        {SUBSCRIPTION_PROVIDERS.map((id) => {
          const st = statusFor(id);
          return (
            <Toggle
              key={id}
              title={
                <span className="provider-title">
                  <span className={`provider-dot ${id}`} aria-hidden />
                  {PROVIDER_LABEL[id]}
                  <span className={`status status-${st.tone}`}>{st.text}</span>
                </span>
              }
              checked={s.enabled_providers.includes(id)}
              onChange={() => toggleProvider(id)}
            />
          );
        })}
      </Section>

      <Section title={t.sectionApiKeyProviders}>
        {API_KEY_PROVIDERS.map((id) => (
          <ApiKeyProviderRow
            key={id}
            id={id}
            t={t}
            enabled={s.enabled_providers.includes(id)}
            hasKey={!!keyStatus[id]}
            onToggle={() => toggleProvider(id)}
            onKeyChange={refreshKeys}
          />
        ))}
      </Section>
    </>
  );
}

export function DisplayTab({ t, s, update }: TabProps) {
  return (
    <>
      <Section title={t.sectionTray}>
        <Row title={t.activeProvider} desc={t.activeProviderDesc}>
          <Select
            value={s.active_provider}
            onChange={(v) => update({ active_provider: v as ActiveProvider })}
            options={[
              { value: "auto", label: t.activeAuto },
              { value: "claude", label: PROVIDER_LABEL.claude },
              { value: "codex", label: PROVIDER_LABEL.codex },
            ]}
          />
        </Row>
        <Row title={t.trayStyle} desc={t.trayStyleDesc}>
          <Select
            value={s.display_style}
            onChange={(v) => update({ display_style: v as DisplayStyle })}
            options={[
              { value: "numbers", label: t.trayNumbers },
              { value: "bars", label: t.trayBars },
            ]}
          />
        </Row>
        <Toggle
          title={t.showRemaining}
          desc={t.showRemainingDesc}
          checked={s.show_remaining}
          onChange={(v) => update({ show_remaining: v })}
        />
      </Section>

      <Section title={t.sectionThresholds}>
        <Row title={t.warnThreshold} value={`${s.thresholds.warn}%`}>
          <input
            className="slider warn"
            type="range"
            min={0}
            max={100}
            value={s.thresholds.warn}
            onChange={(e) =>
              update({
                thresholds: { ...s.thresholds, warn: Number(e.target.value) },
              })
            }
          />
        </Row>
        <Row title={t.dangerThreshold} value={`${s.thresholds.danger}%`}>
          <input
            className="slider danger"
            type="range"
            min={0}
            max={100}
            value={s.thresholds.danger}
            onChange={(e) =>
              update({
                thresholds: { ...s.thresholds, danger: Number(e.target.value) },
              })
            }
          />
        </Row>
      </Section>
    </>
  );
}

export function AdvancedTab({
  t,
  s,
  update,
  replace,
}: TabProps & { replace: (next: SettingsType) => void }) {
  const reset = async () => {
    // Defaults live in one place — `Settings::default()` on the backend. The
    // `reset_settings` command keeps the user's enabled providers, persists, and
    // returns the resulting settings, so the UI never maintains a second copy of
    // the default values (which previously drifted from the Rust struct).
    const next = await resetSettings();
    replace(next);
  };

  return (
    <>
      {isWindows() && (
        <Section title={t.sectionPlatform}>
          <Toggle
            title={t.floatPanel}
            desc={t.floatPanelDesc}
            checked={s.windows_float_panel}
            onChange={(v) => update({ windows_float_panel: v })}
          />
        </Section>
      )}
      <Section title={t.sectionReset}>
        <Row title={t.resetDefaults} desc={t.resetDefaultsDesc}>
          <button className="settings-button danger" onClick={reset}>
            {t.resetButton}
          </button>
        </Row>
      </Section>
    </>
  );
}

export function AboutTab({ t }: { t: Strings }) {
  return (
    <div className="about">
      <div className="about-mark" aria-hidden>
        <GridIcon />
      </div>
      <h2>{t.appName}</h2>
      <p className="about-version">
        {t.version} {APP_VERSION}
      </p>
      <p className="about-blurb">{t.aboutBlurb}</p>
      <p className="about-privacy">{t.privacyFooter}</p>
    </div>
  );
}
