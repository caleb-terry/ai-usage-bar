import {
  PROVIDER_LABEL,
  Settings,
  UsageSnapshot,
  displayPct,
  formatReset,
  thresholdColor,
} from "../api";

interface Props {
  snapshot: UsageSnapshot;
  settings: Settings;
  compact?: boolean;
}

function Metric({
  label,
  utilization,
  resetAt,
  settings,
}: {
  label: string;
  utilization: number;
  resetAt?: string;
  settings: Settings;
}) {
  const pct = displayPct(utilization, settings.show_remaining);
  const reset = formatReset(resetAt);
  // The bar always reflects raw utilization (how full), regardless of the
  // used/remaining label preference.
  return (
    <div className="metric">
      <div className="metric-head">
        <span className="metric-label">{label}</span>
        <span className="metric-value">
          {Math.round(pct)}% {settings.show_remaining ? "left" : "used"}
        </span>
      </div>
      <div className="bar-track">
        <div
          className="bar-fill"
          style={{
            width: `${Math.min(100, Math.max(0, utilization))}%`,
            background: thresholdColor(utilization, settings.thresholds),
          }}
        />
      </div>
      {reset && <span className="metric-reset">resets in {reset}</span>}
    </div>
  );
}

export default function SnapshotCard({ snapshot, settings, compact }: Props) {
  const label = PROVIDER_LABEL[snapshot.provider];
  const mode = snapshot.mode;

  return (
    <section className={`snapshot ${compact ? "compact" : ""}`}>
      <header className="snapshot-head">
        <span className={`provider-dot ${snapshot.provider}`} aria-hidden />
        <span className="snapshot-title">{label}</span>
        {snapshot.plan_label && <span className="pill">{snapshot.plan_label}</span>}
        {snapshot.stale && <span className="stale-badge">stale</span>}
      </header>

      <div className="snapshot-body">
        {mode.kind === "session" && (
          <>
            <Metric
              label={mode.primary.label || "5h"}
              utilization={mode.primary.utilization}
              resetAt={mode.primary.reset_at}
              settings={settings}
            />
            {mode.secondary && (
              <Metric
                label={mode.secondary.label || "Week"}
                utilization={mode.secondary.utilization}
                resetAt={mode.secondary.reset_at}
                settings={settings}
              />
            )}
          </>
        )}

        {mode.kind === "spend_cap" && (
          <Metric
            label="Spend cap"
            utilization={mode.utilization}
            resetAt={mode.reset_at}
            settings={settings}
          />
        )}

        {mode.kind === "unauthenticated" && (
          <p className="empty-state">
            Not signed in. Run <code>{snapshot.provider === "claude" ? "claude" : "codex"}</code>{" "}
            in a terminal to authenticate.
          </p>
        )}

        {mode.kind === "api_key_only" && (
          <p className="empty-state">
            API-key mode — no subscription limits to display.
          </p>
        )}
      </div>

      {!compact && snapshot.extras && (
        <footer className="snapshot-extras">
          {snapshot.extras.credit_balance_cents != null && (
            <span>Credits: {snapshot.extras.credit_balance_cents}</span>
          )}
          {snapshot.extras.on_demand_resets != null && (
            <span>Resets available: {snapshot.extras.on_demand_resets}</span>
          )}
          {snapshot.extras.code_review_utilization != null && (
            <span>
              Code review: {Math.round(snapshot.extras.code_review_utilization)}%
            </span>
          )}
        </footer>
      )}
    </section>
  );
}
