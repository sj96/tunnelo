import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { TunnelProfile, TunnelStatus } from "../types";
import { isWildcardBastionHost } from "../bastionHost";
import { effectiveRemoteScheme, mappingForwardTarget, mappingRemoteUrl } from "../types";

interface Props {
  profile: TunnelProfile;
  status: TunnelStatus;
  resolvedBastionHost?: string | null;
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

export function TunnelCard({
  profile,
  status,
  resolvedBastionHost,
  busy,
  onStart,
  onStop,
  onEdit,
  onDelete,
  onForgetHostKey,
}: Props) {
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
          <span className={`status-dot ${status}`} title={STATUS_LABEL[status]} aria-label={STATUS_LABEL[status]} />
          <span className="tunnel-name">{profile.name}</span>
          <span className={`status-pill ${status}`}>{STATUS_LABEL[status]}</span>
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
            {profile.mappings.map((m) => {
              const accessUrl = mappingRemoteUrl(m);
              const forwardTarget = mappingForwardTarget(m);
              const connected = status === "connected" && m.remoteHost;
              const scheme = effectiveRemoteScheme(m);
              const isBrowserUrl = scheme === "http" || scheme === "https";

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
                        title="Copy URL"
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
