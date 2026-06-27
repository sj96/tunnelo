import { forwardRef, useEffect, useImperativeHandle, useState } from "react";

import type { ForwardMapping } from "../types";
import { mappingForwardTarget } from "../types";
import { parseRemoteTarget, formatForwardInput } from "../urlUtils";

export interface MappingRowHandle {
  commit: () => Partial<ForwardMapping> | false;
}

interface Props {
  index: number;
  mapping: ForwardMapping;
  canRemove: boolean;
  onChange: (patch: Partial<ForwardMapping>) => void;
  onRemove: () => void;
}

const EXAMPLE_HINT =
  "e.g. https://hrm.mservice.com.vn, http://nexus.mservice.com.vn, app.example.com:8080, tcp://172.16.54.37:5432";

export const MappingRow = forwardRef<MappingRowHandle, Props>(function MappingRow(
  { index, mapping, canRemove, onChange, onRemove },
  ref,
) {
  const [remoteInput, setRemoteInput] = useState(() => mappingForwardTarget(mapping));
  const [remoteErr, setRemoteErr] = useState<string | null>(null);
  const [focused, setFocused] = useState(false);

  useEffect(() => {
    setRemoteInput(mappingForwardTarget(mapping));
  }, [mapping.id, mapping.remoteHost, mapping.remotePort, mapping.remoteScheme]);

  function commitRemote(): Partial<ForwardMapping> | false {
    const trimmed = remoteInput.trim();
    if (!trimmed) {
      setRemoteErr("Target required");
      onChange({ remoteHost: "", remotePort: 443, remoteScheme: undefined });
      return false;
    }
    const parsed = parseRemoteTarget(trimmed);
    if (!parsed) {
      setRemoteErr(EXAMPLE_HINT);
      return false;
    }
    setRemoteErr(null);
    const patch = {
      remoteHost: parsed.host,
      remotePort: parsed.port,
      remoteScheme: parsed.scheme,
    };
    onChange(patch);
    setRemoteInput(formatForwardInput(parsed.host, parsed.port, parsed.scheme));
    return patch;
  }

  useImperativeHandle(ref, () => ({ commit: commitRemote }), [remoteInput]);

  const valid = mapping.remoteHost && mapping.remotePort > 0;

  return (
    <div className={`mapping-row ${valid ? "valid" : ""} ${remoteErr ? "invalid" : ""}`}>
      <span className="mapping-index" aria-hidden>
        {index + 1}
      </span>

      <div className="mapping-main">
        <label className="field mapping-col">
          <span className="sr-only">Forward target #{index + 1}</span>
          <input
            value={remoteInput}
            placeholder="https://hrm.mservice.com.vn"
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
          aria-label={`Remove forward #${index + 1}`}
          onClick={onRemove}
        >
          <svg aria-hidden="true" className="mapping-remove-icon" width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path
              d="M2.5 4h11M5.5 4V3a1 1 0 0 1 1-1h3a1 1 0 0 1 1 1v1M6.25 7v4.25M9.75 7v4.25M3.5 4l.65 8.35a1 1 0 0 0 1 .9h5.7a1 1 0 0 0 1-.9L12.5 4"
              stroke="currentColor"
              strokeWidth="1.25"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      )}
    </div>
  );
});

