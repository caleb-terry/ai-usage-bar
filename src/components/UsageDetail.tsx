import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getUsage, refreshNow, UsageReport } from "../api";
import SnapshotCard from "./SnapshotCard";
import "../styles/panel.css";

/// The left-click detail panel (Windows float / macOS popover). Shows the
/// active provider prominently with the other provider beneath it.
export default function UsageDetail() {
  const [report, setReport] = useState<UsageReport | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const load = async () => {
    try {
      setReport(await getUsage());
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
      setReport(await refreshNow());
    } finally {
      setRefreshing(false);
    }
  };

  if (!report) {
    return (
      <div className="panel">
        <div className="panel-loading">Loading usage…</div>
      </div>
    );
  }

  const { snapshots, active, settings } = report;
  const order = active
    ? [active, ...Object.keys(snapshots).filter((p) => p !== active)]
    : Object.keys(snapshots);

  return (
    <div className="panel" data-tauri-drag-region>
      <div className="panel-cards">
        {order.map((id) => {
          const snap = snapshots[id as keyof typeof snapshots];
          return snap ? (
            <SnapshotCard
              key={id}
              snapshot={snap}
              settings={settings}
              compact={id !== active}
            />
          ) : null;
        })}
        {order.length === 0 && (
          <p className="empty-state">No providers enabled.</p>
        )}
      </div>

      <div className="panel-actions">
        <button onClick={onRefresh} disabled={refreshing}>
          {refreshing ? "Refreshing…" : "Refresh"}
        </button>
        <button onClick={() => import("@tauri-apps/api/webviewWindow").then(({ WebviewWindow }) => {
          const w = WebviewWindow.getByLabel("settings");
          w.then((win) => win?.show().then(() => win?.setFocus()));
        })}>
          Settings
        </button>
      </div>
    </div>
  );
}
