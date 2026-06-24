// Mirrors the Rust data model in src-tauri/src/model.rs.

import { formatRemoteUrl } from "./urlUtils";

export type SshAuth =
  | { type: "key"; keyPath: string; hasPassphrase: boolean }
  | { type: "password" }
  | { type: "agent" };

export interface SshConfig {
  host: string;
  port: number;
  user: string;
  auth: SshAuth;
}

export interface ForwardMapping {
  id: string;
  /** Target host as seen from the SSH bastion. */
  remoteHost: string;
  remotePort: number;
}

export interface TunnelProfile {
  id: string;
  name: string;
  ssh: SshConfig;
  mappings: ForwardMapping[];
  autoStart: boolean;
  autoReconnect: boolean;
}

export type LogLevel =
  | "scan"
  | "connect"
  | "auth"
  | "forward"
  | "ready"
  | "warn"
  | "error"
  | "info";

export interface TunnelLogEntry {
  level: LogLevel;
  message: string;
  ts: number;
}

export type TunnelStatus =
  | "stopped"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

export interface TunnelState {
  id: string;
  status: TunnelStatus;
  localUrls?: string[];
  localUrl?: string | null;
  /** Legacy alias — same as localUrls */
  publicUrls?: string[];
  publicUrl?: string | null;
  error: string | null;
  /** Resolved bastion IP when ssh.host is a wildcard pattern. */
  resolvedBastionHost?: string | null;
}

export function emptyMapping(): ForwardMapping {
  return {
    id: "",
    remoteHost: "",
    remotePort: 443,
  };
}

export function emptyProfile(): TunnelProfile {
  return {
    id: "",
    name: "",
    ssh: { host: "", port: 22, user: "", auth: { type: "agent" } },
    mappings: [emptyMapping()],
    autoStart: false,
    autoReconnect: true,
  };
}

/** Public URL the user opens once the tunnel is active. */
export function mappingLocalUrl(m: ForwardMapping): string {
  return mappingRemoteUrl(m);
}

export function mappingRemoteUrl(m: ForwardMapping): string {
  return formatRemoteUrl(m.remoteHost, m.remotePort);
}

export function profileLocalUrls(p: TunnelProfile): string[] {
  return p.mappings
    .filter((m) => m.remoteHost.trim() && m.remotePort > 0)
    .map((m) => mappingRemoteUrl(m));
}
