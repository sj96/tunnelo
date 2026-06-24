import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import type { LogLevel, TunnelLogEntry, TunnelProfile } from "../types";

const LEVEL_LABEL: Record<LogLevel, string> = {
  scan: "SCAN",
  connect: "CONN",
  auth: "AUTH",
  forward: "FWD",
  ready: "READY",
  warn: "WARN",
  error: "ERR",
  info: "INFO",
};

function formatLogTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function normalizeLogEntry(payload: {
  id: string;
  level?: LogLevel;
  message?: string;
  line?: string;
  ts?: number;
}): TunnelLogEntry {
  return {
    level: payload.level ?? "info",
    message: payload.message ?? payload.line ?? "",
    ts: payload.ts ?? Date.now(),
  };
}

export function ConsolePanel() {
  const [tunnels, setTunnels] = useState<TunnelProfile[]>([]);
  const [logs, setLogs] = useState<Record<string, TunnelLogEntry[]>>({});
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const load = () => api.listTunnels().then(setTunnels).catch(() => {});
    load();
    const unLog = listen<{
      id: string;
      level?: LogLevel;
      message?: string;
      line?: string;
      ts?: number;
    }>("tunnel://log", (e) => {
      const { id, ...rest } = e.payload;
      const entry = normalizeLogEntry({ id, ...rest });
      setLogs((prev) => {
        const next = [...(prev[id] ?? []), entry].slice(-500);
        return { ...prev, [id]: next };
      });
    });
    const unProfiles = listen("tunnel://profiles-changed", load);
    return () => {
      unLog.then((f) => f());
      unProfiles.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [logs]);

  const sorted = [...tunnels].sort((a, b) => a.name.localeCompare(b.name));
  const lineCount = sorted.reduce((n, t) => n + (logs[t.id]?.length ?? 0), 0);

  function clearLogs() {
    setLogs({});
  }

  return (
    <section className="console-panel" aria-label="Console">
      <div className="console-panel-head">
        <span className="console-panel-title">Console</span>
        <span className="console-panel-meta">{lineCount} lines</span>
        <button
          type="button"
          className="btn ghost sm"
          disabled={lineCount === 0}
          onClick={clearLogs}
        >
          Clear
        </button>
      </div>
      <div className="console-panel-body" ref={bodyRef}>
        {sorted.flatMap((t) =>
          (logs[t.id] ?? []).map((entry, i) => (
            <div
              className={`log-line log-${entry.level}`}
              key={`${t.id}-${i}`}
            >
              <span className="log-time">{formatLogTime(entry.ts)}</span>
              <span className="log-id">{t.name}</span>
              <span className={`log-level log-level-${entry.level}`}>
                {LEVEL_LABEL[entry.level]}
              </span>
              <span className="log-message">{entry.message}</span>
            </div>
          )),
        )}
        {sorted.every((t) => !(logs[t.id]?.length)) && (
          <div className="log-line muted">
            <span className="log-time">—</span>
            <span className="log-id">—</span>
            <span className="log-level">—</span>
            <span className="log-message">Logs appear when tunnels connect or error.</span>
          </div>
        )}
      </div>
    </section>
  );
}
