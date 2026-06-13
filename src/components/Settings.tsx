import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ActiveProvider,
  DisplayStyle,
  PROVIDER_LABEL,
  ProviderId,
  Settings as SettingsType,
  UsageReport,
  getUsage,
  setSettings as persist,
} from "../api";
import "../styles/settings.css";

const PROVIDERS: ProviderId[] = ["claude", "codex"];

export default function Settings() {
  const [report, setReport] = useState<UsageReport | null>(null);

  const load = async () => setReport(await getUsage());

  useEffect(() => {
    load();
    const un = listen("usage-updated", load);
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!report) return <div className="settings loading">Loading…</div>;

  const s = report.settings;

  // Optimistically update local state, then persist to the backend (which
  // re-renders the tray and emits usage-updated).
  const update = async (patch: Partial<SettingsType>) => {
    const next = { ...s, ...patch };
    setReport({ ...report, settings: next });
    await persist(next);
  };

  const toggleProvider = (id: ProviderId) => {
    const enabled = s.enabled_providers.includes(id)
      ? s.enabled_providers.filter((p) => p !== id)
      : [...s.enabled_providers, id];
    update({ enabled_providers: enabled });
  };

  const statusFor = (id: ProviderId): string => {
    const snap = report.snapshots[id];
    if (!snap) return "Not enabled";
    switch (snap.mode.kind) {
      case "unauthenticated":
        return "Sign-in needed";
      case "api_key_only":
        return "API key mode";
      default:
        return snap.stale ? "Connected (stale)" : "Connected";
    }
  };

  return (
    <div className="settings">
      <h1>AI Usage Bar</h1>

      <section className="group">
        <h2>Providers</h2>
        {PROVIDERS.map((id) => (
          <label key={id} className="row toggle-row">
            <span className="row-main">
              <span className={`provider-dot ${id}`} aria-hidden />
              <span>{PROVIDER_LABEL[id]}</span>
              <span className={`status status-${statusFor(id).split(" ")[0].toLowerCase()}`}>
                {statusFor(id)}
              </span>
            </span>
            <input
              type="checkbox"
              checked={s.enabled_providers.includes(id)}
              onChange={() => toggleProvider(id)}
            />
          </label>
        ))}
      </section>

      <section className="group">
        <h2>Display</h2>

        <label className="row">
          <span>Active provider</span>
          <select
            value={s.active_provider}
            onChange={(e) =>
              update({ active_provider: e.target.value as ActiveProvider })
            }
          >
            <option value="auto">Auto (highest usage)</option>
            <option value="claude">Claude Code</option>
            <option value="codex">Codex</option>
          </select>
        </label>

        <label className="row">
          <span>Tray style</span>
          <select
            value={s.display_style}
            onChange={(e) =>
              update({ display_style: e.target.value as DisplayStyle })
            }
          >
            <option value="numbers">Numbers</option>
            <option value="bars">Progress bars</option>
          </select>
        </label>

        <label className="row toggle-row">
          <span>Show remaining % (instead of used)</span>
          <input
            type="checkbox"
            checked={s.show_remaining}
            onChange={(e) => update({ show_remaining: e.target.checked })}
          />
        </label>
      </section>

      <section className="group">
        <h2>Thresholds</h2>
        <label className="row">
          <span>Warn at (yellow) — {s.thresholds.warn}%</span>
          <input
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
        </label>
        <label className="row">
          <span>Danger at (red) — {s.thresholds.danger}%</span>
          <input
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
        </label>
      </section>

      <section className="group">
        <h2>Polling</h2>
        <label className="row">
          <span>Refresh every</span>
          <select
            value={s.poll_interval_secs}
            onChange={(e) =>
              update({ poll_interval_secs: Number(e.target.value) })
            }
          >
            <option value={60}>1 minute</option>
            <option value={180}>3 minutes</option>
            <option value={300}>5 minutes</option>
            <option value={600}>10 minutes</option>
            <option value={900}>15 minutes</option>
          </select>
        </label>
      </section>

      <section className="group">
        <h2>System</h2>
        <label className="row toggle-row">
          <span>Launch at login</span>
          <input
            type="checkbox"
            checked={s.launch_at_login}
            onChange={(e) => update({ launch_at_login: e.target.checked })}
          />
        </label>
        {isWindows() && (
          <label className="row toggle-row">
            <span>Open floating panel on click</span>
            <input
              type="checkbox"
              checked={s.windows_float_panel}
              onChange={(e) => update({ windows_float_panel: e.target.checked })}
            />
          </label>
        )}
      </section>

      <footer className="settings-footer">
        Credentials are read locally and never sent anywhere except each
        provider's own API.
      </footer>
    </div>
  );
}

function isWindows(): boolean {
  return navigator.userAgent.toLowerCase().includes("windows");
}
