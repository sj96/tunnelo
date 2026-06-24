import { useEffect, useState } from "react";

import type { ForwardMapping } from "../types";
import { mappingRemoteUrl } from "../types";
import { parseRemoteTarget, formatRemoteUrl } from "../urlUtils";

interface Props {
  index: number;
  mapping: ForwardMapping;
  canRemove: boolean;
  onChange: (patch: Partial<ForwardMapping>) => void;
  onRemove: () => void;
}

export function MappingRow({ index, mapping, canRemove, onChange, onRemove }: Props) {
  const [remoteInput, setRemoteInput] = useState(() => mappingRemoteUrl(mapping));
  const [remoteErr, setRemoteErr] = useState<string | null>(null);
  const [focused, setFocused] = useState(false);

  useEffect(() => {
    setRemoteInput(mappingRemoteUrl(mapping));
  }, [mapping.id, mapping.remoteHost, mapping.remotePort]);

  function commitRemote() {
    const trimmed = remoteInput.trim();
    if (!trimmed) {
      setRemoteErr("URL required");
      onChange({ remoteHost: "", remotePort: 443 });
      return;
    }
    const parsed = parseRemoteTarget(trimmed);
    if (!parsed) {
      setRemoteErr("e.g. https://gitlab.example.com");
      return;
    }
    setRemoteErr(null);
    onChange({ remoteHost: parsed.host, remotePort: parsed.port });
    setRemoteInput(formatRemoteUrl(parsed.host, parsed.port));
  }

  const valid = mapping.remoteHost && mapping.remotePort > 0;

  return (
    <div className={`mapping-row ${valid ? "valid" : ""} ${remoteErr ? "invalid" : ""}`}>
      <span className="mapping-index" aria-hidden>
        {index + 1}
      </span>

      <div className="mapping-main">
        <label className="field mapping-col">
          <span className="sr-only">Remote URL #{index + 1}</span>
          <input
            value={remoteInput}
            placeholder="https://gitlab.example.com"
            aria-invalid={!!remoteErr}
            onChange={(e) => {
              setRemoteInput(e.target.value);
              setRemoteErr(null);
            }}
            onFocus={() => setFocused(true)}
            onBlur={() => {
              setFocused(false);
              commitRemote();
            }}
            onKeyDown={(e) => e.key === "Enter" && commitRemote()}
          />
          {remoteErr && <span className="field-hint err">{remoteErr}</span>}
          {!remoteErr && valid && !focused && (
            <span className="field-hint ok">
              → {mapping.remoteHost}:{mapping.remotePort} via bastion
            </span>
          )}
        </label>
      </div>

      {canRemove && (
        <button
          type="button"
          className="icon-btn mapping-remove"
          title="Remove forward"
          onClick={onRemove}
        >
          ×
        </button>
      )}
    </div>
  );
}
