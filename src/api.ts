// Thin typed wrapper over Tauri IPC commands.

import { invoke } from "@tauri-apps/api/core";
import type { TunnelProfile } from "./types";

export const api = {
  listTunnels: () => invoke<TunnelProfile[]>("list_tunnels"),
  getTunnel: (id: string) => invoke<TunnelProfile | null>("get_tunnel", { id }),
  saveTunnel: (profile: TunnelProfile) =>
    invoke<TunnelProfile>("save_tunnel", { profile }),
  deleteTunnel: (id: string) => invoke<void>("delete_tunnel", { id }),
  startTunnel: (id: string) => invoke<void>("start_tunnel", { id }),
  stopTunnel: (id: string) => invoke<void>("stop_tunnel", { id }),
  tunnelRunning: (id: string) => invoke<boolean>("tunnel_running", { id }),
  setSecret: (id: string, kind: string, value: string) =>
    invoke<void>("set_secret", { id, kind, value }),
  deleteSecret: (id: string, kind: string) =>
    invoke<void>("delete_secret", { id, kind }),
  hasSecret: (id: string, kind: string) =>
    invoke<boolean>("has_secret", { id, kind }),
  listHostKeys: () => invoke<Record<string, string>>("list_host_keys"),
  forgetHostKey: (host: string, port: number, tunnelId?: string) =>
    invoke<void>("forget_host_key", { host, port, tunnelId }),
  defaultSshKeyPath: () => invoke<string>("default_ssh_key_path"),
};
