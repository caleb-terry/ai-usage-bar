import { ReactNode, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ActiveProvider,
  DisplayStyle,
  Language,
  PROVIDER_LABEL,
  ProviderId,
  Settings as SettingsType,
  TerminalApp,
  UsageReport,
  formatRelative,
  getUsage,
  setSettings as persist,
} from "../api";
import { strings, Strings } from "../i18n";
import "../styles/settings.css";

const PROVIDERS: ProviderId[] = ["claude", "codex"];
const APP_VERSION = "0.1.0";

type Tab = "general" | "providers" | "display" | "advanced" | "about";

export default function Settings() {
  const [report, setReport] = useState<UsageReport | null>(null);
  const [tab, setTab] = useState<Tab>("general");

  const load = async () => setReport(await getUsage());

  useEffect(() => {
    load();
    const un = listen("usage-updated", load);
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!report) {
    return <div className="settings loading">Loading…</div>;
  }

  const s = report.settings;
  const t = strings(s.language);

  // Optimistically update local state, then persist to the backend (which
  // re-renders the tray and emits usage-updated).
  const update = async (patch: Partial<SettingsType>) => {
    const next = { ...s, ...patch };
    setReport({ ...report, settings: next });
    await persist(next);
  };

  const tabs: { id: Tab; label: string; icon: ReactNode }[] = [
    { id: "general", label: t.tabGeneral, icon: <GearIcon /> },
    { id: "providers", label: t.tabProviders, icon: <GridIcon /> },
    { id: "display", label: t.tabDisplay, icon: <EyeIcon /> },
    { id: "advanced", label: t.tabAdvanced, icon: <SlidersIcon /> },
    { id: "about", label: t.tabAbout, icon: <InfoIcon /> },
  ];

  return (
    <div className="settings">
      <nav className="settings-tabs" data-tauri-drag-region>
        {tabs.map((tb) => (
          <button
            key={tb.id}
            className={`settings-tab ${tab === tb.id ? "active" : ""}`}
            onClick={() => setTab(tb.id)}
          >
            <span className="settings-tab-icon">{tb.icon}</span>
            <span className="settings-tab-label">{tb.label}</span>
          </button>
        ))}
      </nav>

      <div className="settings-body">
        {tab === "general" && (
          <GeneralTab t={t} s={s} report={report} update={update} />
        )}
        {tab === "providers" && (
          <ProvidersTab t={t} s={s} report={report} update={update} />
        )}
        {tab === "display" && <DisplayTab t={t} s={s} update={update} />}
        {tab === "advanced" && <AdvancedTab t={t} s={s} update={update} />}
        {tab === "about" && <AboutTab t={t} />}
      </div>
    </div>
  );
}

// ---- tabs ----

interface TabProps {
  t: Strings;
  s: SettingsType;
  update: (patch: Partial<SettingsType>) => void;
}

function GeneralTab({
  t,
  s,
  report,
  update,
}: TabProps & { report: UsageReport }) {
  const cadenceLabel = CADENCE_OPTIONS.find(
    (o) => o.secs === s.poll_interval_secs,
  );
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
                  "10m",
                )}
              </div>
              {PROVIDERS.map((id) => {
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
            options={CADENCE_OPTIONS.map((o) => ({
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

function ProvidersTab({
  t,
  s,
  report,
  update,
}: TabProps & { report: UsageReport }) {
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
    <Section title={t.sectionConnections}>
      {PROVIDERS.map((id) => {
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
  );
}

function DisplayTab({ t, s, update }: TabProps) {
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

function AdvancedTab({ t, s, update }: TabProps) {
  const reset = async () => {
    // Keep the user's enabled providers; reset everything else to defaults.
    await update({
      active_provider: "auto",
      display_style: "numbers",
      show_remaining: false,
      poll_interval_secs: 180,
      thresholds: { warn: 50, danger: 80 },
      language: "system",
      default_terminal: "terminal",
      show_cost_summary: true,
      cost_history_days: 30,
      check_provider_status: true,
      session_quota_notifications: true,
      quota_warning_notifications: true,
    });
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

function AboutTab({ t }: { t: Strings }) {
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

// ---- reusable controls ----

const CADENCE_OPTIONS = [
  { secs: 60, label: "1 min" },
  { secs: 180, label: "3 min" },
  { secs: 300, label: "5 min" },
  { secs: 600, label: "10 min" },
  { secs: 900, label: "15 min" },
];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <h3 className="settings-section-title">{title}</h3>
      <div className="settings-card">{children}</div>
    </section>
  );
}

function Row({
  title,
  desc,
  value,
  children,
}: {
  title: ReactNode;
  desc?: string;
  value?: string;
  children?: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <div className="settings-row-title">
          {title}
          {value && <span className="settings-row-value">{value}</span>}
        </div>
        {desc && <div className="settings-row-desc">{desc}</div>}
      </div>
      {children && <div className="settings-row-control">{children}</div>}
    </div>
  );
}

function Toggle({
  title,
  desc,
  checked,
  onChange,
}: {
  title: ReactNode;
  desc?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="settings-row interactive">
      <span className={`settings-check ${checked ? "on" : ""}`} aria-hidden>
        {checked && <CheckIcon />}
      </span>
      <div className="settings-row-text">
        <div className="settings-row-title">{title}</div>
        {desc && <div className="settings-row-desc">{desc}</div>}
      </div>
      <input
        type="checkbox"
        className="visually-hidden"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

function Select({
  value,
  options,
  onChange,
}: {
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <div className="settings-select">
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronIcon />
    </div>
  );
}

function Stepper({
  value,
  min,
  max,
  step,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
}) {
  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  return (
    <div className="settings-stepper">
      <button
        aria-label="Increase"
        disabled={value >= max}
        onClick={() => onChange(clamp(value + step))}
      >
        <ChevronIcon up />
      </button>
      <button
        aria-label="Decrease"
        disabled={value <= min}
        onClick={() => onChange(clamp(value - step))}
      >
        <ChevronIcon />
      </button>
    </div>
  );
}

function isWindows(): boolean {
  return navigator.userAgent.toLowerCase().includes("windows");
}

// ---- icons ----

function CheckIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" aria-hidden>
      <path
        d="M5 13l4 4L19 7"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ChevronIcon({ up }: { up?: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
      style={up ? { transform: "rotate(180deg)" } : undefined}
    >
      <path
        d="M6 9l6 6 6-6"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.8" />
      <path
        d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
    </svg>
  );
}

function GridIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <rect x="3" y="3" width="7.5" height="7.5" rx="1.8" />
      <rect x="13.5" y="3" width="7.5" height="7.5" rx="1.8" />
      <rect x="3" y="13.5" width="7.5" height="7.5" rx="1.8" />
      <rect x="13.5" y="13.5" width="7.5" height="7.5" rx="1.8" />
    </svg>
  );
}

function EyeIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
      <path
        d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinejoin="round"
      />
      <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.8" />
    </svg>
  );
}

function SlidersIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
      <path
        d="M4 7h10M18 7h2M4 17h2M10 17h10"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
      <circle cx="16" cy="7" r="2.2" stroke="currentColor" strokeWidth="1.8" />
      <circle cx="8" cy="17" r="2.2" stroke="currentColor" strokeWidth="1.8" />
    </svg>
  );
}

function InfoIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" />
      <path
        d="M12 11v5"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
      />
      <circle cx="12" cy="8" r="1.1" fill="currentColor" />
    </svg>
  );
}
