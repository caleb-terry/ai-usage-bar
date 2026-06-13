// English strings for the settings UI. The keys here define the canonical
// string set; additional locales implement the same `Strings` shape.

export const en = {
  appName: "AI Usage Bar",

  // Tabs
  tabGeneral: "General",
  tabProviders: "Providers",
  tabDisplay: "Display",
  tabAdvanced: "Advanced",
  tabAbout: "About",

  loading: "Loading…",

  // Sections
  sectionSystem: "System",
  sectionUsage: "Usage",
  sectionAutomation: "Automation",
  sectionConnections: "Connections",
  sectionTray: "Tray",
  sectionThresholds: "Thresholds",
  sectionPlatform: "Platform",
  sectionReset: "Reset",

  // General · System
  language: "Language",
  languageDesc: "Change the display language. Requires app restart to take full effect.",
  languageSystem: "System",
  languageEnglish: "English",
  defaultTerminal: "Default Terminal",
  defaultTerminalDesc: "Terminal used by the Open Terminal action.",
  startAtLogin: "Start at Login",
  startAtLoginDesc: "Automatically opens AI Usage Bar when you start your Mac.",

  // General · Usage
  showCostSummary: "Show cost summary",
  showCostSummaryDesc:
    "Reads local usage logs. Shows today + the selected history window in the menu.",
  historyWindow: "History window",
  historyWindowValue: (days: number) => `${days} days`,
  autoRefreshLine: (cadence: string, timeout: string) =>
    `Auto-refresh: ${cadence} · Timeout: ${timeout}`,
  providerUpdatedLine: (provider: string, ago: string, days: number, cost?: string) =>
    cost
      ? `${provider}: Updated ${ago} · ${days}d ${cost}`
      : `${provider}: Updated ${ago} · ${days}d`,
  never: "never",

  // General · Automation
  refreshCadence: "Refresh cadence",
  refreshCadenceDesc: "How often AI Usage Bar polls providers in the background.",
  checkProviderStatus: "Check provider status",
  checkProviderStatusDesc:
    "Polls provider status pages, surfacing incidents in the icon and menu.",
  sessionQuotaNotifications: "Session quota notifications",
  sessionQuotaNotificationsDesc:
    "Notifies when the 5-hour session quota hits 0% and when it becomes available again.",
  quotaWarningNotifications: "Quota warning notifications",
  quotaWarningNotificationsDesc:
    "Warns when session or weekly quota remaining crosses configured thresholds.",

  // Cadence options
  cadence1m: "1 min",
  cadence3m: "3 min",
  cadence5m: "5 min",
  cadence10m: "10 min",
  cadence15m: "15 min",

  // Providers
  enableProvider: "Enable",
  statusNotEnabled: "Not enabled",
  statusSignIn: "Sign-in needed",
  statusApiKey: "API key mode",
  statusConnected: "Connected",
  statusConnectedStale: "Connected (stale)",

  // Display
  activeProvider: "Active provider",
  activeProviderDesc: "Which provider the single tray icon shows.",
  activeAuto: "Auto (highest usage)",
  trayStyle: "Tray style",
  trayStyleDesc: "How usage is drawn in the menu bar.",
  trayNumbers: "Numbers",
  trayBars: "Progress bars",
  showRemaining: "Show remaining %",
  showRemainingDesc: "Display remaining percentage instead of used.",
  warnThreshold: "Warn at (yellow)",
  dangerThreshold: "Danger at (red)",

  // Advanced
  floatPanel: "Open floating panel on click",
  floatPanelDesc: "Left-click the tray icon to open the panel instead of settings.",
  resetDefaults: "Reset to defaults",
  resetDefaultsDesc: "Restore every setting to its original value.",
  resetButton: "Reset",

  // About
  version: "Version",
  aboutBlurb:
    "A lightweight menu-bar monitor for your Claude Code and OpenAI Codex usage.",
  privacyFooter:
    "Credentials are read locally and never sent anywhere except each provider's own API.",
};

export type Strings = typeof en;
