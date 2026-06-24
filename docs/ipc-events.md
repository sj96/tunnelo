# IPC & Events

## Tauri Commands (React → Rust)

Gọi qua `invoke()` — wrapper: `src/api.ts`.

| Command | Mô tả |
|---|---|
| `list_tunnels` | Danh sách profiles |
| `get_tunnel(id)` | Một profile |
| `save_tunnel(profile)` | Upsert profile |
| `delete_tunnel(id)` | Stop + xóa secrets + xóa profile |
| `start_tunnel(id)` | Bắt đầu SSH + routing |
| `stop_tunnel(id)` | Dừng tunnel + deactivate router |
| `tunnel_running(id)` | `bool` |
| `set_secret(id, kind, value)` | Lưu keyring |
| `delete_secret(id, kind)` | Xóa một secret |
| `has_secret(id, kind)` | Kiểm tra tồn tại |
| `list_host_keys` | Map fingerprints |
| `forget_host_key(host, port, tunnelId?)` | Xóa TOFU entry |
| `default_ssh_key_path` | Path mặc định `id_ed25519` |

Đăng ký handler: `lib.rs` → `generate_handler![...]`.

## Events (Rust → React)

Listen trong `App.tsx` qua `@tauri-apps/api/event`.

### `tunnel://state`

Payload `TunnelState`:

```ts
{
  id: string;
  status: "stopped" | "connecting" | "connected" | "reconnecting" | "error";
  localUrls?: string[];
  localUrl?: string;           // backward compat
  publicUrls?: string;          // alias localUrls
  error?: string | null;
  resolvedBastionHost?: string | null;
}
```

Emit từ `tunnel.rs::emit_state` — mỗi chuyển trạng thái (connecting, connected, error, stopped…).

### `tunnel://log`

```ts
{ id: string; level: string; message: string; ts: number }
```

Levels: `scan`, `connect`, `auth`, `forward`, `ready`, `warn`, `error`, `info`.

`ConsolePanel` hiển thị theo tunnel id.

## Luồng điển hình Start

```
UI: api.startTunnel(id)
  → start_tunnel
  → TunnelManager::start (async)
  → emit connecting
  → activate_tunnel
  → connect + forwards
  → emit connected + localUrls
UI: listen tunnel://state → cập nhật status, URL, resolved bastion
```

## Source files

| File | Vai trò |
|---|---|
| `commands.rs` | Command handlers |
| `tunnel.rs` | `emit_state`, `emit_log` |
| `model.rs` | `TunnelState`, `TunnelStatus` |
| `src/api.ts` | Typed invoke |
| `src/App.tsx` | Event listeners |
| `src/components/ConsolePanel.tsx` | Log UI |

## Gotchas

- Lỗi command trả `String` (`.map_err(e)`).
- `stop_tunnel` emit `stopped` ngay; task async có thể còn dọn dẹp vài giây.
- `busy` state UI reset khi nhận `tunnel://state`.
- Không có polling status — UI dựa hoàn toàn vào events + `tunnel_running` lúc refresh.
