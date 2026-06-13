import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  CostSummary,
  PROVIDER_ACCENT,
  PROVIDER_LABEL,
  PROVIDER_TAB_LABEL,
  ProviderId,
  getCostSummary,
  getUsage,
  openTerminal,
  refreshNow,
  severityColor,
  UsageReport,
} from "../api";
import CostFooter from "./CostFooter";
import ProviderGlyph from "./ProviderGlyph";
import SnapshotCard from "./SnapshotCard";
import "../styles/panel.css";

type Tab = "overview" | ProviderId;

/// The left-click detail panel (Windows float / macOS popover). A tab bar
/// switches between an Overview (all providers stacked) and a focused view of
/// each enabled provider rendered as a brand-colored hero card.
export default function UsageDetail() {
  const [report, setReport] = useState<UsageReport | null>(null);
  const [cost, setCost] = useState<CostSummary | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [tab, setTab] = useState<Tab>("overview");

  const load = async () => {
    try {
      const [r, c] = await Promise.all([getUsage(), getCostSummary()]);
      setReport(r);
      setCost(c);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    load();
    const un = listen("usage-updated", load);
    return () => {
      un.then((f) => f());
    };
  }, []);

  const onRefresh = async () => {
    setRefreshing(true);
    try {
      const [r, c] = await Promise.all([refreshNow(), getCostSummary()]);
      setReport(r);
      setCost(c);
    } finally {
      setRefreshing(false);
    }
  };

  // Provider order: the active provider leads so Overview shows it as the hero.
  const providers = useMemo<ProviderId[]>(() => {
    if (!report) return [];
    const present = Object.keys(report.snapshots) as ProviderId[];
    const { active } = report;
    return active
      ? [active, ...present.filter((p) => p !== active)]
      : present;
  }, [report]);

  // Keep the selected tab valid if the provider set changes under us.
  useEffect(() => {
    if (tab !== "overview" && !providers.includes(tab)) setTab("overview");
  }, [providers, tab]);

  if (!report) {
    return (
      <div className="panel">
        <div className="panel-loading">Loading usage…</div>
      </div>
    );
  }

  const { snapshots, settings } = report;
  const tabs: Tab[] = ["overview", ...providers];
  // Active incidents, worst first, so the banner leads with the most serious.
  const rank = { critical: 3, major: 2, minor: 1, none: 0 } as const;
  const activeIncidents = (report.incidents ?? [])
    .filter((i) => i.severity !== "none")
    .sort((a, b) => rank[b.severity] - rank[a.severity]);

  return (
    <div className="panel" data-tauri-drag-region>
      <nav className="tab-bar">
        {tabs.map((t) => {
          const isActive = t === tab;
          const accent =
            t === "overview" ? "var(--text)" : PROVIDER_ACCENT[t];
          return (
            <button
              key={t}
              className={`tab ${isActive ? "active" : ""}`}
              onClick={() => setTab(t)}
              style={{ ["--tab-accent" as string]: accent }}
            >
              <span className="tab-icon">
                {t === "overview" ? <OverviewGlyph /> : <ProviderGlyph provider={t} />}
              </span>
              <span className="tab-text">
                {t === "overview" ? "Overview" : PROVIDER_TAB_LABEL[t]}
              </span>
              <span className="tab-underline" />
            </button>
          );
        })}
      </nav>

      <div className="panel-scroll">
        {activeIncidents.length > 0 && (
          <div className="incident-banner">
            {activeIncidents.map((i) => (
              <div
                key={i.provider}
                className="incident-row"
                style={{ ["--incident-color" as string]: severityColor(i.severity) }}
              >
                <span className="incident-dot" />
                <span className="incident-text">
                  {PROVIDER_LABEL[i.provider]}: {i.description}
                </span>
              </div>
            ))}
          </div>
        )}
        {tab === "overview" ? (
          <div className="panel-cards">
            {providers.map((id, i) => {
              const snap = snapshots[id];
              return snap ? (
                <SnapshotCard
                  key={id}
                  snapshot={snap}
                  settings={settings}
                  hero={i === 0}
                />
              ) : null;
            })}
            {providers.length === 0 && (
              <p className="empty-state">No providers enabled.</p>
            )}
            {cost && providers.length > 0 && <CostFooter summary={cost} />}
          </div>
        ) : (
          <div className="panel-cards">
            {snapshots[tab] && (
              <SnapshotCard
                snapshot={snapshots[tab]!}
                settings={settings}
                hero
              />
            )}
            {cost && <CostFooter summary={cost} focus={tab} />}
          </div>
        )}
      </div>

      <div className="panel-actions">
        <button onClick={onRefresh} disabled={refreshing}>
          {refreshing ? "Refreshing…" : "Refresh"}
        </button>
        <button onClick={() => openTerminal()}>Terminal</button>
        <button
          onClick={() =>
            import("@tauri-apps/api/webviewWindow").then(
              ({ WebviewWindow }) => {
                const w = WebviewWindow.getByLabel("settings");
                w.then((win) => win?.show().then(() => win?.setFocus()));
              },
            )
          }
        >
          Settings
        </button>
      </div>
    </div>
  );
}

/// A simple grid mark for the Overview tab, matching the reference's 2×2 icon.
function OverviewGlyph() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <rect x="3" y="3" width="7.5" height="7.5" rx="1.6" />
      <rect x="13.5" y="3" width="7.5" height="7.5" rx="1.6" />
      <rect x="3" y="13.5" width="7.5" height="7.5" rx="1.6" />
      <rect x="13.5" y="13.5" width="7.5" height="7.5" rx="1.6" />
    </svg>
  );
}
