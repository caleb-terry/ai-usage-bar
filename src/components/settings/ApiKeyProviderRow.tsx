// A single API-key provider row for the Providers tab. Stateful (draft key +
// saving flag), so it lives in its own module rather than bloating tabs.tsx.

import { useState } from "react";
import { PROVIDER_LABEL, ProviderId, setApiKey } from "../../api";
import { Strings } from "../../i18n";
import { Toggle } from "./controls";

/// A single API-key provider: enable toggle + a password-style key field that
/// shows whether a key is already stored (without ever revealing it). The stored
/// state comes from the parent (one shared apiKeyStatus call); saving/clearing a
/// key calls back up so the parent re-reads the shared status.
export function ApiKeyProviderRow({
  id,
  t,
  enabled,
  hasKey,
  onToggle,
  onKeyChange,
}: {
  id: ProviderId;
  t: Strings;
  enabled: boolean;
  hasKey: boolean;
  onToggle: () => void;
  onKeyChange: () => void;
}) {
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);

  const save = async (key: string) => {
    setSaving(true);
    try {
      await setApiKey(id, key);
      setDraft("");
      onKeyChange();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="apikey-row">
      <Toggle
        title={
          <span className="provider-title">
            <span className={`provider-dot ${id}`} aria-hidden />
            {PROVIDER_LABEL[id]}
            <span className={`status status-${hasKey ? "ok" : "dim"}`}>
              {hasKey ? t.statusKeyStored : t.statusNoKey}
            </span>
          </span>
        }
        checked={enabled}
        onChange={onToggle}
      />
      <div className="apikey-input">
        <input
          type="password"
          placeholder={hasKey ? "••••••••  (stored)" : t.apiKeyPlaceholder}
          value={draft}
          disabled={saving}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && draft.trim()) save(draft.trim());
          }}
        />
        <button
          disabled={saving || !draft.trim()}
          onClick={() => save(draft.trim())}
        >
          {t.apiKeySave}
        </button>
        {hasKey && (
          <button
            className="apikey-clear"
            disabled={saving}
            onClick={() => save("")}
          >
            {t.apiKeyClear}
          </button>
        )}
      </div>
    </div>
  );
}
