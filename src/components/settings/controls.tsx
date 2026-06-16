// Reusable presentational controls for the settings UI: section/row scaffolding
// plus the toggle, select, and stepper inputs. Kept backend-agnostic — they only
// take values and callbacks.

import { ReactNode } from "react";
import { Strings } from "../../i18n";
import { CheckIcon, ChevronIcon } from "./icons";

/// Background poll cadences offered in the General tab. The seconds are the
/// canonical value; labels are localized via the i18n string set so they never
/// drift from the rest of the UI's wording.
export function cadenceOptions(t: Strings): { secs: number; label: string }[] {
  return [
    { secs: 60, label: t.cadence1m },
    { secs: 180, label: t.cadence3m },
    { secs: 300, label: t.cadence5m },
    { secs: 600, label: t.cadence10m },
    { secs: 900, label: t.cadence15m },
  ];
}

export function isWindows(): boolean {
  return navigator.userAgent.toLowerCase().includes("windows");
}

export function Section({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-section">
      <h3 className="settings-section-title">{title}</h3>
      <div className="settings-card">{children}</div>
    </section>
  );
}

export function Row({
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

export function Toggle({
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

export function Select({
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

export function Stepper({
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
