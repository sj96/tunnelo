import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { TunnelProfile, TunnelStatus } from "../types";
import { isWildcardBastionHost } from "../bastionHost";
import { effectiveRemoteScheme, mappingForwardTarget, mappingLocalUrl } from "../types";
import { isWebForward } from "../urlUtils";

interface Props {
  profile: TunnelProfile;
  status: TunnelStatus;
  /** Runtime local access URLs from backend (one per mapping, connected only). */
  localUrls?: string[];
  resolvedBastionHost?: string | null;
  retryAtMs?: number | null;
  busy: boolean;
  onStart: () => void;
  onStop: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onForgetHostKey: () => void;
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
  busy,
  onStart,
  onStop,
  onEdit,
  onDelete,
  onForgetHostKey,
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

  async function copyUrl(url: string) {
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      /* clipboard unavailable */
    }
  }

  return (
    <article className={`tunnel-row${menuOpen ? " menu-open" : ""}`} role="listitem">
      <div className="tunnel-row-body">
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

        {profile.mappings.length > 0 && (
          <div className="tunnel-row-forwards">
            {profile.mappings.map((m, index) => {
              const accessUrl =
                status === "connected" && localUrls?.[index]
                  ? localUrls[index]
                  : mappingLocalUrl(m);
              const forwardTarget = mappingForwardTarget(m);
              const connected = status === "connected" && m.remoteHost;
              const scheme = effectiveRemoteScheme(m);
              const isBrowserUrl = isWebForward(scheme);
              const copyTitle = isBrowserUrl ? "Copy URL" : "Copy local address";

              return (
                <span className="forward-chip" key={m.id || m.remoteHost}>
                  {connected ? (
                    <>
                      {isBrowserUrl ? (
                        <button type="button" className="forward-link" onClick={() => openUrl(accessUrl)}>
                          {accessUrl}
                        </button>
                      ) : (
                        <span className="forward-link forward-link-static">{accessUrl}</span>
                      )}
                      <button
                        type="button"
                        className="icon-btn icon-btn-sm"
                        title={copyTitle}
                        onClick={() => copyUrl(accessUrl)}
                      >
                        ⧉
                      </button>
                    </>
                  ) : (
                    <span className="forward-idle">{forwardTarget || "…"}</span>
                  )}
                </span>
              );
            })}
          </div>
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

