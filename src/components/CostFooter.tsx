import {
  CostSummary,
  PROVIDER_ACCENT,
  PROVIDER_TAB_LABEL,
  ProviderId,
  formatTokens,
  formatUsd,
} from "../api";

/// Local cost summary strip shown at the foot of the panel. Mirrors CodexBar's
/// local cost scan: a headline "today" figure plus the rolling history-window
/// total, with an optional per-provider breakdown. Rendered only when the user
/// has the cost summary enabled (the summary is null otherwise).
export default function CostFooter({
  summary,
  focus,
}: {
  summary: CostSummary;
  /// When set, show only this provider's figures (focused provider tab).
  focus?: ProviderId;
}) {
  if (focus) {
    const pc = summary.providers[focus];
    if (!pc) return null;
    return (
      <div className="cost-footer" style={{ ["--cost-accent" as string]: PROVIDER_ACCENT[focus] }}>
        <div className="cost-row">
          <span className="cost-label">Today</span>
          <span className="cost-value">{formatUsd(pc.today_usd)}</span>
          <span className="cost-tokens">{formatTokens(pc.today_tokens)} tok</span>
        </div>
        <div className="cost-row">
          <span className="cost-label">{summary.window_days}-day</span>
          <span className="cost-value">{formatUsd(pc.window_usd)}</span>
          <span className="cost-tokens">{formatTokens(pc.window_tokens)} tok</span>
        </div>
      </div>
    );
  }

  const providers = Object.keys(summary.providers) as ProviderId[];
  return (
    <div className="cost-footer">
      <div className="cost-row cost-total">
        <span className="cost-label">Cost today</span>
        <span className="cost-value">{formatUsd(summary.total_today_usd)}</span>
        <span className="cost-tokens">
          {summary.window_days}-day {formatUsd(summary.total_window_usd)}
        </span>
      </div>
      {providers.length > 1 &&
        providers.map((id) => {
          const pc = summary.providers[id]!;
          return (
            <div
              key={id}
              className="cost-row cost-breakdown"
              style={{ ["--cost-accent" as string]: PROVIDER_ACCENT[id] }}
            >
              <span className="cost-dot" />
              <span className="cost-label">{PROVIDER_TAB_LABEL[id]}</span>
              <span className="cost-value">{formatUsd(pc.today_usd)}</span>
              <span className="cost-tokens">{formatUsd(pc.window_usd)}</span>
            </div>
          );
        })}
    </div>
  );
}
