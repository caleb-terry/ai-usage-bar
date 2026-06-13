import {
  PROVIDER_ACCENT,
  PROVIDER_LABEL,
  Settings,
  UsageSnapshot,
  WindowUsage,
  displayPct,
  formatReset,
} from "../api";
import ProviderGlyph from "./ProviderGlyph";

interface Props {
  snapshot: UsageSnapshot;
  settings: Settings;
  /// When true the card renders as a filled, brand-colored hero (the active or
  /// selected provider). When false it's a quieter card for the Overview list.
  hero?: boolean;
}

/// A two-line metric row matching the reference layout: a labelled bar with the
/// headline percentage on the left and the reset timing on the right.
function Metric({
  label,
  utilization,
  resetAt,
  settings,
  hero,
}: {
  label: string;
  utilization: number;
  resetAt?: string;
  settings: Settings;
  hero?: boolean;
}) {
  const pct = Math.round(displayPct(utilization, settings.show_remaining));
  const reset = formatReset(resetAt);
  const fill = Math.min(100, Math.max(0, utilization));

  return (
    <div className="metric">
      <span className="metric-label">{label}</span>
      <div className={`bar-track ${hero ? "on-hero" : ""}`}>
        <div
          className="bar-fill"
          style={{
            width: `${fill}%`,
            // On the hero card the bar is white-on-tint; on plain cards it uses
            // the provider accent so colour still telegraphs the provider.
            background: hero ? "var(--hero-fg)" : "var(--card-accent)",
          }}
        />
      </div>
      <div className="metric-foot">
        <span className="metric-value">
          {pct}% {settings.show_remaining ? "left" : "used"}
        </span>
        {reset && <span className="metric-reset">Resets in {reset}</span>}
      </div>
    </div>
  );
}

function windowRows(snapshot: UsageSnapshot): WindowUsage[] {
  const mode = snapshot.mode;
  if (mode.kind === "session") {
    return [mode.primary, ...(mode.secondary ? [mode.secondary] : [])];
  }
  return [];
}

export default function SnapshotCard({ snapshot, settings, hero }: Props) {
  const label = PROVIDER_LABEL[snapshot.provider];
  const accent = PROVIDER_ACCENT[snapshot.provider];
  const mode = snapshot.mode;
  const rows = windowRows(snapshot);

  // CSS custom props let one stylesheet theme every card from the provider
  // accent without per-provider class soup.
  const style = {
    ["--card-accent" as string]: accent,
    ...(hero ? { ["--hero-bg" as string]: accent } : {}),
  } as React.CSSProperties;

  return (
    <section
      className={`snapshot ${hero ? "hero" : "plain"}`}
      style={style}
    >
      <header className="snapshot-head">
        <span className="snapshot-mark">
          <ProviderGlyph provider={snapshot.provider} size={hero ? 20 : 16} />
        </span>
        <span className="snapshot-title">{label}</span>
        {snapshot.plan_label && (
          <span className="snapshot-plan">{snapshot.plan_label}</span>
        )}
        {snapshot.stale && <span className="stale-badge">stale</span>}
      </header>

      <div className="snapshot-body">
        {mode.kind === "session" &&
          rows.map((w, i) => (
            <Metric
              key={i}
              label={w.label || (i === 0 ? "Session" : "Weekly")}
              utilization={w.utilization}
              resetAt={w.reset_at}
              settings={settings}
              hero={hero}
            />
          ))}

        {mode.kind === "spend_cap" && (
          <Metric
            label="Spend cap"
            utilization={mode.utilization}
            resetAt={mode.reset_at}
            settings={settings}
            hero={hero}
          />
        )}

        {mode.kind === "unauthenticated" &&
          (snapshot.provider === "claude" || snapshot.provider === "codex" ? (
            <p className="empty-state">
              Not signed in. Run <code>{snapshot.provider}</code> in a terminal
              to authenticate.
            </p>
          ) : (
            <p className="empty-state">
              No API key. Add one in Settings → Providers.
            </p>
          ))}

        {mode.kind === "api_key_only" && (
          <p className="empty-state">
            API-key mode — no subscription limits to display.
          </p>
        )}
      </div>

      {snapshot.extras &&
        (snapshot.extras.credit_balance_cents != null ||
          snapshot.extras.on_demand_resets != null ||
          snapshot.extras.code_review_utilization != null ||
          snapshot.extras.extra_usage_cap_cents != null) && (
          <footer className="snapshot-extras">
            {snapshot.extras.extra_usage_cap_cents != null && (
              <span>
                <em>Extra usage</em>$
                {((snapshot.extras.extra_usage_used_cents ?? 0) / 100).toFixed(
                  2,
                )}{" "}
                / ${(snapshot.extras.extra_usage_cap_cents / 100).toFixed(2)}
              </span>
            )}
            {snapshot.extras.credit_balance_cents != null && (
              <span>
                <em>Credits</em>$
                {(snapshot.extras.credit_balance_cents / 100).toFixed(2)}
              </span>
            )}
            {snapshot.extras.on_demand_resets != null && (
              <span>
                <em>Resets avail.</em>
                {snapshot.extras.on_demand_resets}
              </span>
            )}
            {snapshot.extras.code_review_utilization != null && (
              <span>
                <em>Code review</em>
                {Math.round(snapshot.extras.code_review_utilization)}%
              </span>
            )}
          </footer>
        )}
    </section>
  );
}
