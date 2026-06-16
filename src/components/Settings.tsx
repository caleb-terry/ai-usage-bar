import { ReactNode, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Settings as SettingsType,
  UsageReport,
  getUsage,
  setSettings as persist,
} from "../api";
import { strings } from "../i18n";
import {
  AboutTab,
  AdvancedTab,
  DisplayTab,
  GeneralTab,
  ProvidersTab,
} from "./settings/tabs";
import {
  EyeIcon,
  GearIcon,
  GridIcon,
  InfoIcon,
  SlidersIcon,
} from "./settings/icons";
import "../styles/settings.css";

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

  // Adopt a settings object the backend already persisted (e.g. reset), without
  // a second round-trip through `persist`.
  const replace = (next: SettingsType) => {
    setReport({ ...report, settings: next });
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
        {tab === "advanced" && (
          <AdvancedTab t={t} s={s} update={update} replace={replace} />
        )}
        {tab === "about" && <AboutTab t={t} />}
      </div>
    </div>
  );
}
