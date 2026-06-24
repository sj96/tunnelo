import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { SshAuth, TunnelProfile } from "../types";
import { emptyMapping } from "../types";
import { isValidBastionHost } from "../bastionHost";
import { MappingRow } from "./MappingRow";
import { api } from "../api";

export interface SecretInput {
  kind: "password" | "passphrase";
  value: string;
}

interface Props {
  initial: TunnelProfile;
  onSave: (p: TunnelProfile, secrets?: SecretInput[]) => void;
  onCancel: () => void;
  standalone?: boolean;
}

const AUTH_LABELS: Record<SshAuth["type"], string> = {
  agent: "Agent",
  key: "Key",
  password: "Password",
};

function validationIssues(p: TunnelProfile): string[] {
  const issues: string[] = [];
  if (!p.name.trim()) issues.push("Name");
  if (!p.ssh.host.trim()) issues.push("Bastion host");
  else if (!isValidBastionHost(p.ssh.host)) issues.push("Bastion host (invalid IPv4 or pattern)");
  if (!p.ssh.user.trim()) issues.push("SSH user");
  if (p.ssh.auth.type === "key" && !p.ssh.auth.keyPath.trim()) issues.push("Key path");
  if (p.mappings.length === 0) issues.push("At least one forward");
  for (let i = 0; i < p.mappings.length; i++) {
    const m = p.mappings[i];
    if (!m.remoteHost.trim() || m.remotePort <= 0) {
      issues.push(`Forward #${i + 1} URL`);
    }
  }
  return issues;
}

export function TunnelForm({ initial, onSave, onCancel, standalone = false }: Props) {
  const [p, setP] = useState<TunnelProfile>(() => ({
    ...initial,
    mappings: initial.mappings.length ? initial.mappings : [emptyMapping()],
  }));
  const [secret, setSecret] = useState("");
  const [showSecret, setShowSecret] = useState(false);
  const editing = !!initial.id;
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
  }, []);

  useEffect(() => {
    if (editing) return;
    api.defaultSshKeyPath().then((keyPath) => {
      setP((prev) => {
        if (prev.ssh.auth.type !== "key" || prev.ssh.auth.keyPath) return prev;
        return {
          ...prev,
          ssh: { ...prev.ssh, auth: { type: "key", keyPath, hasPassphrase: false } },
        };
      });
    });
  }, [editing]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  const set = (patch: Partial<TunnelProfile>) => setP((prev) => ({ ...prev, ...patch }));
  const setSsh = (patch: Partial<TunnelProfile["ssh"]>) =>
    setP((prev) => ({ ...prev, ssh: { ...prev.ssh, ...patch } }));

  const setMapping = (index: number, patch: Partial<TunnelProfile["mappings"][0]>) => {
    setP((prev) => ({
      ...prev,
      mappings: prev.mappings.map((m, i) => (i === index ? { ...m, ...patch } : m)),
    }));
  };

  const addMapping = () => {
    setP((prev) => ({ ...prev, mappings: [...prev.mappings, emptyMapping()] }));
  };

  const removeMapping = (index: number) => {
    setP((prev) => {
      if (prev.mappings.length <= 1) return prev;
      return { ...prev, mappings: prev.mappings.filter((_, i) => i !== index) };
    });
  };

  const setAuthType = (type: SshAuth["type"]) => {
    setSecret("");
    let auth: SshAuth;
    if (type === "key") auth = { type, keyPath: "", hasPassphrase: false };
    else if (type === "password") auth = { type };
    else auth = { type };
    setSsh({ auth });
  };

  const issues = useMemo(() => validationIssues(p), [p]);
  const valid = issues.length === 0;

  async function browseKeyPath() {
    if (p.ssh.auth.type !== "key") return;
    const selected = await open({
      multiple: false,
      directory: false,
      title: "Select SSH private key",
      defaultPath: p.ssh.auth.keyPath || undefined,
      filters: [
        { name: "Key files", extensions: ["pem", "ppk", "key"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof selected === "string") {
      setSsh({
        auth: {
          type: "key",
          keyPath: selected,
          hasPassphrase: p.ssh.auth.hasPassphrase,
        },
      });
    }
  }

  function handleSave() {
    if (!valid) return;
    const profile: TunnelProfile = { ...p };
    const secrets: SecretInput[] = [];
    if (secret) {
      if (p.ssh.auth.type === "password") {
        secrets.push({ kind: "password", value: secret });
      } else if (p.ssh.auth.type === "key") {
        secrets.push({ kind: "passphrase", value: secret });
        profile.ssh = {
          ...p.ssh,
          auth: { type: "key", keyPath: p.ssh.auth.keyPath, hasPassphrase: true },
        };
      }
    }
    onSave(profile, secrets.length ? secrets : undefined);
  }

  const form = (
    <div
      className="modal modal-form"
      role={standalone ? undefined : "dialog"}
      aria-labelledby="tunnel-form-title"
      onClick={standalone ? undefined : (e) => e.stopPropagation()}
    >
      <header className="modal-header">
        <div className="modal-header-text">
          <h2 id="tunnel-form-title">{p.id ? "Edit Tunnel" : "New Tunnel"}</h2>
          <p className="modal-subtitle">
            Configure bastion SSH and remote URLs — local routing is automatic.
          </p>
        </div>
        {!standalone && (
          <button type="button" className="icon-btn modal-close" onClick={onCancel} title="Close (Esc)">
            ×
          </button>
        )}
      </header>

      <div className="modal-body">
        <section className="form-section">
          <h3 className="form-section-title">General</h3>
          <div className="form-section-card">
            <label className="form-row">
              <span className="form-row-label">Name</span>
              <input
                ref={nameRef}
                className="form-row-input"
                value={p.name}
                placeholder="Required"
                onChange={(e) => set({ name: e.target.value })}
              />
            </label>
          </div>
        </section>

        <section className="form-section">
          <h3 className="form-section-title">SSH Bastion</h3>
          <div className="form-section-card">
            <label className="form-row">
              <span className="form-row-label">Host</span>
              <input
                className="form-row-input"
                value={p.ssh.host}
                placeholder="192.168.1.11 hoặc 192.168.1.*"
                onChange={(e) => setSsh({ host: e.target.value })}
              />
            </label>
            <p className="form-footnote">
              Pattern <code>*</code> sẽ tự quét subnet để tìm bastion SSH.
            </p>
            <label className="form-row">
              <span className="form-row-label">Port</span>
              <input
                className="form-row-input form-row-input-narrow"
                type="text"
                inputMode="numeric"
                autoComplete="off"
                value={p.ssh.port}
                onChange={(e) => {
                  const digits = e.target.value.replace(/\D/g, "").slice(0, 5);
                  setSsh({ port: digits ? Math.min(65535, parseInt(digits, 10)) : 22 });
                }}
              />
            </label>
            <label className="form-row">
              <span className="form-row-label">User</span>
              <input
                className="form-row-input"
                value={p.ssh.user}
                placeholder="user-01"
                onChange={(e) => setSsh({ user: e.target.value })}
              />
            </label>

            <div className="form-row form-row-static">
              <span className="form-row-label">Auth</span>
              <div className="auth-segment" role="radiogroup" aria-label="SSH authentication">
                {(["agent", "key", "password"] as const).map((type) => (
                  <button
                    key={type}
                    type="button"
                    role="radio"
                    aria-checked={p.ssh.auth.type === type}
                    className={`auth-segment-btn ${p.ssh.auth.type === type ? "active" : ""}`}
                    onClick={() => setAuthType(type)}
                  >
                    {AUTH_LABELS[type]}
                  </button>
                ))}
              </div>
            </div>

            {p.ssh.auth.type === "key" && (
              <div className="auth-fields">
                <label className="form-row form-row-stack">
                  <span className="form-row-label">Key Path</span>
                  <div className="input-with-action">
                    <input
                      className="form-row-input form-row-input-full"
                      value={p.ssh.auth.keyPath}
                      placeholder="~/.ssh/id_ed25519"
                      onChange={(e) =>
                        setSsh({
                          auth: {
                            type: "key",
                            keyPath: e.target.value,
                            hasPassphrase: p.ssh.auth.type === "key" && p.ssh.auth.hasPassphrase,
                          },
                        })
                      }
                    />
                    <button
                      type="button"
                      className="input-action input-action-wide"
                      onClick={() => void browseKeyPath()}
                      title="Browse for private key file"
                    >
                      Browse
                    </button>
                  </div>
                </label>
                <p className="form-footnote">
                  OpenSSH keys (<code>id_ed25519</code>, <code>id_rsa</code>, <code>.pem</code>).{" "}
                  <code>~</code> home paths supported.
                  <br />
                  Browse — choose <strong>All files</strong> for extensionless keys (e.g.{" "}
                  <code>id_ed25519</code>).
                </p>
                <label className="form-row form-row-stack">
                  <span className="form-row-label">
                    Passphrase {editing ? "(leave blank to keep)" : "(optional)"}
                  </span>
                  <div className="input-with-action">
                    <input
                      className="form-row-input form-row-input-full"
                      type={showSecret ? "text" : "password"}
                      value={secret}
                      placeholder="optional"
                      onChange={(e) => setSecret(e.target.value)}
                    />
                    <button
                      type="button"
                      className="input-action"
                      onClick={() => setShowSecret((v) => !v)}
                    >
                      {showSecret ? "Hide" : "Show"}
                    </button>
                  </div>
                </label>
              </div>
            )}

            {p.ssh.auth.type === "password" && (
              <div className="auth-fields">
                <label className="form-row form-row-stack">
                  <span className="form-row-label">
                    Password {editing ? "(leave blank to keep)" : ""}
                  </span>
                  <div className="input-with-action">
                    <input
                      className="form-row-input form-row-input-full"
                      type={showSecret ? "text" : "password"}
                      value={secret}
                      onChange={(e) => setSecret(e.target.value)}
                    />
                    <button
                      type="button"
                      className="input-action"
                      onClick={() => setShowSecret((v) => !v)}
                    >
                      {showSecret ? "Hide" : "Show"}
                    </button>
                  </div>
                </label>
              </div>
            )}

            {p.ssh.auth.type === "agent" && (
              <p className="form-footnote">Uses keys loaded in ssh-agent or Pageant.</p>
            )}
          </div>
        </section>

        <section className="form-section">
          <div className="form-section-head">
            <h3 className="form-section-title">Port Forwards</h3>
            <span className="form-section-badge">{p.mappings.length}</span>
          </div>
          <p className="form-footnote">Remote service URLs reachable from the bastion.</p>
          <div className="form-section-card">
            <div className="mapping-list">
              {p.mappings.map((m, i) => (
                <MappingRow
                  key={m.id || `new-${i}`}
                  index={i}
                  mapping={m}
                  canRemove={p.mappings.length > 1}
                  onChange={(patch) => setMapping(i, patch)}
                  onRemove={() => removeMapping(i)}
                />
              ))}
            </div>
            <button type="button" className="btn ghost add-mapping" onClick={addMapping}>
              Add Forward
            </button>
          </div>
        </section>

        <section className="form-section">
          <h3 className="form-section-title">Behavior</h3>
          <div className="form-section-card toggle-group">
            <label className="toggle-row">
              <span className="toggle-row-body">
                <strong>Auto-Reconnect</strong>
                <span>Restore tunnel after disconnect</span>
              </span>
              <span className="ios-switch">
                <input
                  type="checkbox"
                  checked={p.autoReconnect}
                  onChange={(e) => set({ autoReconnect: e.target.checked })}
                />
                <span className="ios-switch-track" />
                <span className="ios-switch-thumb" />
              </span>
            </label>
            <label className="toggle-row">
              <span className="toggle-row-body">
                <strong>Start on Launch</strong>
                <span>Connect when app opens</span>
              </span>
              <span className="ios-switch">
                <input
                  type="checkbox"
                  checked={p.autoStart}
                  onChange={(e) => set({ autoStart: e.target.checked })}
                />
                <span className="ios-switch-track" />
                <span className="ios-switch-thumb" />
              </span>
            </label>
          </div>
        </section>
      </div>

      <footer className="modal-footer">
        {!valid && (
          <p className="validation-hint" role="status">
            Missing: {issues.join(", ")}
          </p>
        )}
        <div className="modal-actions">
          <button type="button" className="btn ghost" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="btn primary" disabled={!valid} onClick={handleSave}>
            Save
          </button>
        </div>
      </footer>
    </div>
  );

  if (standalone) {
    return <div className="form-window-shell">{form}</div>;
  }

  return (
    <div className="modal-backdrop">
      {form}
    </div>
  );
}
