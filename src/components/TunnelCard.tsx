import { useEffect, useRef, useState } from "react";
import type { TunnelProfile, TunnelStatsEvent, TunnelStatus } from "../types";
import { isWildcardBastionHost } from "../bastionHost";
import { mappingForwardTarget, mappingLocalUrl } from "../types";
import { TunnelMonitor } from "./TunnelMonitor";
import type { TrafficPoint } from "./TunnelMonitor";

interface Props {
  profile: TunnelProfile;
  status: TunnelStatus;
  /** Runtime local access URLs from backend (one per mapping, connected only). */
  localUrls?: string[];
  resolvedBastionHost?: string | null;
  retryAtMs?: number | null;
  stats?: TunnelStatsEvent;
  statsHistory?: Record<string, TrafficPoint[]>;
  aggregateHistory?: TrafficPoint[];
  busy: boolean;
  onStart: () => void;
  onStop: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onForgetHostKey: () => void;
  onTogglePin: (mappingId: string) => void;
}

const STATUS_LABEL: Record<TunnelStatus, string> = {
  stopped: "Stopped",
  connecting: "Connecting…",
  connected: "Connected",
  reconnecting: "Reconnecting…",
  error: "Error",
};

function useRetryCountdown(retryAtMs: number | null | undefined): number {
  const [secsLeft, setSecsLeft] = useState(0);

  useEffect(() => {
    if (retryAtMs == null) {
      setSecsLeft(0);
      return;
    }

    function tick() {
      const remaining = Math.max(0, Math.ceil((retryAtMs! - Date.now()) / 1000));
      setSecsLeft(remaining);
    }

    tick();
    const id = setInterval(tick, 250);
    return () => clearInterval(id);
  }, [retryAtMs]);

  return secsLeft;
}

function reconnectLabel(retryAtMs: number | null | undefined, secsLeft: number): string {
  if (retryAtMs == null || secsLeft <= 0) {
    return STATUS_LABEL.reconnecting;
  }
  return `Reconnecting… · retry in ${secsLeft}s`;
}

export function TunnelCard({
  profile,
  status,
  localUrls,
  resolvedBastionHost,
  retryAtMs,
  stats,
  statsHistory,
  aggregateHistory,
  busy,
  onStart,
  onStop,
  onEdit,
  onDelete,
  onForgetHostKey,
  onTogglePin,
}: Props) {
  const retrySecsLeft = useRetryCountdown(status === "reconnecting" ? retryAtMs : null);
  const statusLabel =
    status === "reconnecting"
      ? reconnectLabel(retryAtMs, retrySecsLeft)
      : STATUS_LABEL[status];
  const running = status === "connected" || status === "connecting" || status === "reconnecting";
  const showResolved =
    status === "connected" &&
    isWildcardBastionHost(profile.ssh.host) &&
    resolvedBastionHost;
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    function onPointerDown(e: PointerEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    }
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [menuOpen]);

  const orderedMappings = profile.mappings
    .map((mapping, index) => ({ mapping, index }))
    .sort((a, b) => Number(b.mapping.pinned) - Number(a.mapping.pinned));
  const aggregateRates = stats?.mappings.reduce(
    (total, mapping) => ({
      down: total.down + mapping.bytesDownPerSec,
      up: total.up + mapping.bytesUpPerSec,
    }),
    { down: 0, up: 0 },
  );

  return (
    <article
      className={`tunnel-row${profile.mappings.length > 0 ? " has-forwards" : ""}${menuOpen ? " menu-open" : ""}`}
      role="listitem"
    >
      <div className="tunnel-row-body">
        <div className="tunnel-header">
        <div className="tunnel-row-head">
          <span className={`status-dot ${status}`} title={statusLabel} aria-label={statusLabel} />
          <span className="tunnel-name">{profile.name}</span>
          <span className={`status-pill ${status}`}>
            {status === "reconnecting" && retrySecsLeft > 0 ? (
              <>
                Reconnecting… · retry in{" "}
                <span className="retry-countdown">{retrySecsLeft}s</span>
              </>
            ) : (
              statusLabel
            )}
          </span>
        </div>

        <p className="tunnel-row-bastion" title={`${profile.ssh.user}@${profile.ssh.host}:${profile.ssh.port}`}>
          <code>{profile.ssh.user}@{profile.ssh.host}</code>
          {profile.ssh.port != 22 && <span className="port-tag">:{profile.ssh.port}</span>}
          {showResolved && (
            <span className="resolved-bastion" title="Resolved bastion IP">
              {" → "}
              <code>{resolvedBastionHost}</code>
            </span>
          )}
        </p>
        </div>

        {status === "connected" && (
          <TunnelMonitor
            aggregate
            history={aggregateHistory ?? []}
            aggregateRates={aggregateRates}
          />
        )}

        {profile.mappings.length > 0 && (
          <section className="tunnel-forward-section" aria-label="Configured forwards">
            <div className="tunnel-forward-section-head">
              <strong>Forwards</strong>
              <span>{profile.mappings.length}</span>
            </div>
            <div className="tunnel-forward-list">
              {orderedMappings.map(({ mapping: m, index }) => {
                const accessUrl = localUrls?.[index] ?? mappingLocalUrl(m);
                const forwardTarget = mappingForwardTarget(m);
                const mappingKey = m.id || `${m.remoteHost}:${m.remotePort}`;
                const snapshot = stats?.mappings.find((item) => item.mappingId === m.id);
                const stateLabel =
                  status === "connected"
                    ? undefined
                    : status === "connecting"
                      ? "Connecting…"
                      : status === "reconnecting"
                        ? "Reconnecting…"
                        : status === "error"
                          ? "Unavailable"
                          : "Not connected";
                return (
                  <div className="tunnel-forward-group" key={mappingKey}>
                    <div className={`tunnel-forward-item${status === "connected" ? "" : " idle"}`}>
                      <div className="forward-item-target">
                        <span className={`forward-status-dot ${snapshot ? "active" : ""}`} />
                        <div>
                          <strong title={forwardTarget}>{forwardTarget || "…"}</strong>
                          <span title={accessUrl}>{accessUrl}</span>
                        </div>
                      </div>
                      {status === "connected" && (
                        <TunnelMonitor
                          snapshot={snapshot}
                          history={statsHistory?.[m.id] ?? []}
                        />
                      )}
                      <span className="forward-item-connections">
                        {snapshot ? `${snapshot.activeConnections} conn.` : stateLabel ?? "—"}
                      </span>
                      <div className="forward-item-actions">
                        <span
                          className={`forward-pin${m.pinned ? " pinned" : ""}`}
                          role="button"
                          tabIndex={0}
                          aria-pressed={m.pinned}
                          aria-label={m.pinned ? "Unpin forward" : "Pin forward"}
                          title={m.pinned ? "Unpin forward" : "Pin forward"}
                          onClick={(event) => {
                            event.stopPropagation();
                            onTogglePin(m.id);
                          }}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              event.stopPropagation();
                              onTogglePin(m.id);
                            }
                          }}
                        >
                          {m.pinned ? "★" : "☆"}
                        </span>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        )}

      </div>

      <div className="tunnel-row-actions">
        {running ? (
          <button className="btn warn sm" disabled={busy} onClick={onStop} title="Stop tunnel">
            Stop
          </button>
        ) : (
          <button className="btn primary sm" disabled={busy} onClick={onStart} title="Start tunnel">
            Connect
          </button>
        )}
        <button className="icon-btn icon-btn-sm" disabled={running} title="Edit" onClick={onEdit}>
          ✎
        </button>
        <div className="menu-wrap" ref={menuRef}>
          <button
            type="button"
            className="icon-btn icon-btn-sm"
            disabled={running}
            title="More"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((v) => !v)}
          >
            ⋯
          </button>
          {menuOpen && (
            <div className="dropdown-menu" role="menu">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false);
                  onForgetHostKey();
                }}
              >
                Forget host key
              </button>
              <button
                type="button"
                role="menuitem"
                className="danger"
                onClick={() => {
                  setMenuOpen(false);
                  onDelete();
                }}
              >
                Delete
              </button>
            </div>
          )}
        </div>
      </div>
    </article>
  );
}

