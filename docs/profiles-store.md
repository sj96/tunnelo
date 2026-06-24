# Profiles Store

## User-facing

Mỗi tunnel là một **profile** lưu trên đĩa: tên, bastion SSH, danh sách forward, `autoStart`, `autoReconnect`. Secrets **không** nằm trong file này.

## Vị trí file

```
<app-data>/tunnels.json
```

Windows: `%APPDATA%\com.tunnelo.app\` (Tauri app data dir).

## Schema (hiện tại)

```json
{
  "id": "uuid",
  "name": "GitLab via bastion",
  "ssh": {
    "host": "192.168.1.11",
    "port": 22,
    "user": "dev",
    "auth": { "type": "agent" }
  },
  "mappings": [
    {
      "id": "uuid",
      "remoteHost": "gitlab.example.com",
      "remotePort": 443
    }
  ],
  "autoStart": false,
  "autoReconnect": true
}
```

`auth.type`: `agent` | `key` | `password`.

Key auth: `{ "type": "key", "keyPath": "~/.ssh/id_ed25519", "hasPassphrase": true }`.

## Luồng kỹ thuật

- `ProfileStore::load` — đọc JSON, `TunnelProfile::normalize` mỗi profile.
- `upsert` — insert/update by id, gán uuid nếu thiếu, persist atomic (`.tmp` + rename).
- `delete` — xóa profile (secrets xóa riêng trong `delete_tunnel`).

## Migration từ phiên bản cũ

`normalize()` chuyển schema legacy khi load:

| Legacy | Chuyển thành |
|---|---|
| `localService` + `public.subdomain` | `mappings[].remoteHost` / `remotePort` |
| `publicHost`, `subdomain` + `baseDomain` | `remoteHost` |
| `localHost` / `localPort` (bind cũ) | `remoteHost` / `remotePort` |
| `manageCaddy` | Bỏ qua (không còn dùng) |

Field legacy **không serialize** lại (`skip_serializing`) — file sau save chỉ còn schema mới.

> **Ghi chú migration:** Phiên bản trước dùng Caddy / reverse tunnel (`-R`). Hiện tại chỉ SSH `-L` + local router. Profile cũ được migrate tự động; không cần Caddy trên server.

## Source files

| File | Vai trò |
|---|---|
| `store.rs` | Load/save/list/upsert/delete |
| `model.rs` | `TunnelProfile`, `normalize`, `ForwardMapping` |
| `commands.rs` | `list_tunnels`, `save_tunnel`, … |
| `src/types.ts` | TypeScript mirror |

## Gotchas

- **Không lưu `localHost`/`localPort`** — bind do `LocalRouter` quyết định runtime.
- **autoStart** — profiles có flag này được `start` ngay trong `lib.rs::setup`.
- File corrupt → parse lỗi khi load app.
- Xóa tunnel đang chạy → `stop` trước rồi xóa file + secrets.
