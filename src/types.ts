// Mirrors the Rust data model in src-tauri/src/model.rs.

import { formatAccessUrl, formatForwardInput, isIpAddress } from "./urlUtils";
import type { RemoteScheme } from "./urlUtils";

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
  /** http/https when parsed from a URL; null for bare host:port (e.g. IP:5432). */
  remoteScheme?: RemoteScheme;
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

/** Infer scheme for legacy profiles missing remoteScheme. */
export function effectiveRemoteScheme(m: ForwardMapping): RemoteScheme {
  if (m.remoteScheme !== undefined) return m.remoteScheme;
  const host = m.remoteHost.trim();
  if (!host) return null;
  if (m.remotePort === 80) return "http";
  if (isIpAddress(host)) return null;
  if (m.remotePort === 443) return "https";
  return "https";
}

/** Public URL the user opens once the tunnel is active. */
export function mappingLocalUrl(m: ForwardMapping): string {
  return mappingRemoteUrl(m);
}

export function mappingForwardTarget(m: ForwardMapping): string {
  return formatForwardInput(m.remoteHost, m.remotePort, effectiveRemoteScheme(m));
}

export function mappingRemoteUrl(m: ForwardMapping): string {
  return formatAccessUrl(m.remoteHost, m.remotePort, effectiveRemoteScheme(m));
}

export function profileLocalUrls(p: TunnelProfile): string[] {
  return p.mappings
    .filter((m) => m.remoteHost.trim() && m.remotePort > 0)
    .map((m) => mappingRemoteUrl(m));
}
