import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { ConsolePanel } from "./components/ConsolePanel";
import { TunnelCard } from "./components/TunnelCard";
import { TunnelForm, type SecretInput } from "./components/TunnelForm";
import {
  emptyProfile,
  type TunnelProfile,
  type TunnelState,
  type TunnelStatus,
} from "./types";
import "./App.css";

const CONSOLE_HEIGHT_KEY = "tunnelo.consoleHeight";
const DEFAULT_CONSOLE_HEIGHT = 200;
const MIN_CONSOLE_HEIGHT = 100;
const MAX_CONSOLE_RATIO = 0.75;

function readConsoleHeight(): number {
  try {
    const saved = localStorage.getItem(CONSOLE_HEIGHT_KEY);
    if (saved) {
      const n = Number(saved);
      if (Number.isFinite(n) && n >= MIN_CONSOLE_HEIGHT) return n;
    }
  } catch {
    // ignore
  }
  return DEFAULT_CONSOLE_HEIGHT;
}

function App() {
  const [consoleHeight, setConsoleHeight] = useState(readConsoleHeight);
  const [resizing, setResizing] = useState(false);
  const [tunnels, setTunnels] = useState<TunnelProfile[]>([]);
  const [statuses, setStatuses] = useState<Record<string, TunnelStatus>>({});
  const [resolvedBastions, setResolvedBastions] = useState<Record<string, string | null>>({});
  const [localUrls, setLocalUrls] = useState<Record<string, string[]>>({});
  const [retryAtMs, setRetryAtMs] = useState<Record<string, number | null>>({});
  const [busy, setBusy] = useState<Record<string, boolean>>({});
  const [editing, setEditing] = useState<TunnelProfile | null>(null);
  const [toast, setToast] = useState<{ msg: string; info?: boolean } | null>(null);
  const mainAreaRef = useRef<HTMLDivElement>(null);
  const consoleHeightRef = useRef(consoleHeight);

  useEffect(() => {
    consoleHeightRef.current = consoleHeight;
  }, [consoleHeight]);

  async function refresh() {
    const list = await api.listTunnels();
    setTunnels(list);

    const runningMap: Record<string, boolean> = {};
    for (const t of list) {
      runningMap[t.id] = await api.tunnelRunning(t.id);
    }

    setStatuses((prev) => {
      const next: Record<string, TunnelStatus> = {};
      for (const t of list) {
        next[t.id] = runningMap[t.id] ? "connected" : (prev[t.id] ?? "stopped");
      }
      return next;
    });
  }

  useEffect(() => {
    refresh().catch((e) => notify(String(e)));

    const unState = listen<TunnelState>("tunnel://state", (e) => {
      const s = e.payload;
      setStatuses((prev) => ({ ...prev, [s.id]: s.status }));
      if (s.resolvedBastionHost !== undefined) {
        setResolvedBastions((prev) => ({ ...prev, [s.id]: s.resolvedBastionHost ?? null }));
      }
      if (s.localUrls !== undefined) {
        setLocalUrls((prev) => ({ ...prev, [s.id]: s.localUrls ?? [] }));
      }
      if (s.status === "stopped" || s.status === "error") {
        setLocalUrls((prev) => {
          const { [s.id]: _, ...rest } = prev;
          return rest;
        });
      }
      if (s.status === "reconnecting") {
        setRetryAtMs((prev) => ({ ...prev, [s.id]: s.retryAtMs ?? null }));
      } else {
        setRetryAtMs((prev) => ({ ...prev, [s.id]: null }));
      }
      setBusy((b) => ({ ...b, [s.id]: false }));
      if (s.status === "error" && s.error) {
        notify(s.error);
      }
    });
    const unProfiles = listen("tunnel://profiles-changed", () => {
      refresh().catch((e) => notify(String(e)));
    });
    return () => {
      unState.then((f) => f());
      unProfiles.then((f) => f());
    };
  }, []);

  function notify(msg: string, info = false) {
    setToast({ msg, info });
    setTimeout(() => setToast(null), 5000);
  }

  function startConsoleResize(e: React.PointerEvent<HTMLDivElement>) {
    e.preventDefault();
    const startY = e.clientY;
    const startHeight = consoleHeightRef.current;
    const area = mainAreaRef.current;
    if (!area) return;

    setResizing(true);

    const onMove = (ev: PointerEvent) => {
      const maxH = area.clientHeight * MAX_CONSOLE_RATIO;
      const next = Math.min(
        maxH,
        Math.max(MIN_CONSOLE_HEIGHT, startHeight + (startY - ev.clientY)),
      );
      setConsoleHeight(next);
    };

    const onUp = () => {
      setResizing(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
      try {
        localStorage.setItem(CONSOLE_HEIGHT_KEY, String(consoleHeightRef.current));
      } catch {
        // ignore
      }
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }

  async function handleSave(p: TunnelProfile, secrets?: SecretInput[]) {
    try {
      const saved = await api.saveTunnel(p);
      for (const s of secrets ?? []) {
        if (s.value) {
          await api.setSecret(saved.id, s.kind, s.value);
        }
      }
      setEditing(null);
      await refresh();
    } catch (e) {
      notify(String(e));
    }
  }

  async function handleDelete(id: string) {
    if (!confirm("Delete this tunnel?")) return;
    try {
      await api.deleteTunnel(id);
      await refresh();
    } catch (e) {
      notify(String(e));
    }
  }

  async function handleStart(id: string) {
    setBusy((b) => ({ ...b, [id]: true }));
    setStatuses((s) => ({ ...s, [id]: "connecting" }));
    try {
      await api.startTunnel(id);
    } catch (e) {
      setStatuses((s) => ({ ...s, [id]: "error" }));
      setBusy((b) => ({ ...b, [id]: false }));
      notify(String(e));
    }
  }

  async function handleStop(id: string) {
    setBusy((b) => ({ ...b, [id]: true }));
    try {
      await api.stopTunnel(id);
      setStatuses((s) => ({ ...s, [id]: "stopped" }));
    } catch (e) {
      notify(String(e));
    } finally {
      setBusy((b) => ({ ...b, [id]: false }));
    }
  }

  async function handleForgetHostKey(profile: TunnelProfile) {
    try {
      await api.forgetHostKey(profile.ssh.host, profile.ssh.port, profile.id);
      notify(`Forgot SSH host key for ${profile.ssh.host}:${profile.ssh.port}`, true);
    } catch (e) {
      notify(String(e));
    }
  }

  const sortedTunnels = [...tunnels].sort((a, b) => a.name.localeCompare(b.name));
  const connectedCount = Object.values(statuses).filter((s) => s === "connected").length;
  const mappingCount = tunnels.reduce((n, t) => n + t.mappings.length, 0);

  return (
    <div className={`app-shell ${resizing ? "is-resizing" : ""}`}>
      <header className="toolbar">
        <div className="toolbar-brand">
          <span className="brand-icon" aria-hidden>⛓</span>
          <div className="toolbar-titles">
            <h1 className="toolbar-title">Tunnels</h1>
            {tunnels.length > 0 && (
              <p className="toolbar-subtitle">
                {connectedCount}/{tunnels.length} connected · {mappingCount} forwards
              </p>
            )}
          </div>
        </div>

        <button className="btn primary sm" onClick={() => setEditing(emptyProfile())}>
          New
        </button>
      </header>

      <div className="main-area" ref={mainAreaRef}>
        <main className="content">
          {tunnels.length === 0 ? (
            <div className="empty">
              <span className="empty-icon" aria-hidden>⛓</span>
              <p className="empty-title">No Tunnels</p>
              <p className="empty-sub">
                Forward ports through an SSH bastion with <code>-L</code> — reach internal services
                like GitLab at <code>127.0.0.1:443</code>.
              </p>
              <button className="btn primary" onClick={() => setEditing(emptyProfile())}>
                New Tunnel
              </button>
            </div>
          ) : (
            <section className="list-section" aria-label="Tunnel list">
              <div className="list-group" role="list">
                {sortedTunnels.map((t) => (
                  <TunnelCard
                    key={t.id}
                    profile={t}
                    status={statuses[t.id] ?? "stopped"}
                    localUrls={localUrls[t.id]}
                    resolvedBastionHost={resolvedBastions[t.id]}
                    retryAtMs={retryAtMs[t.id]}
                    busy={!!busy[t.id]}
                    onStart={() => handleStart(t.id)}
                    onStop={() => handleStop(t.id)}
                    onEdit={() => setEditing(t)}
                    onDelete={() => handleDelete(t.id)}
                    onForgetHostKey={() => handleForgetHostKey(t)}
                  />
                ))}
              </div>
            </section>
          )}
        </main>

        <div
          className={`split-handle ${resizing ? "dragging" : ""}`}
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize console"
          onPointerDown={startConsoleResize}
        />
        <div className="console-pane" style={{ height: consoleHeight }}>
          <ConsolePanel />
        </div>
      </div>

      {editing && (
        <TunnelForm
          initial={editing}
          onSave={handleSave}
          onCancel={() => setEditing(null)}
        />
      )}

      {toast && <div className={`toast ${toast.info ? "info" : ""}`}>{toast.msg}</div>}
    </div>
  );
}

export default App;
