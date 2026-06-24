# Host Keys (TOFU)

## User-facing

Lần đầu kết nối bastion mới, app **lưu fingerprint** SSH server key (Trust On First Use). Lần sau so khớp — nếu server đổi key (reinstall, MITM) → lỗi, tunnel không connect.

**Forget host key** trên menu thẻ tunnel → xóa fingerprint, cho phép TOFU lại lần sau.

## Format lưu trữ

File: `<app-data>/host_keys.json`

```json
{
  "192.168.1.11:22": "sha256:AbCdEf..."
}
```

Key = `host:port`. Fingerprint OpenSSH SHA256 (lowercase `sha256:`).

## Luồng kỹ thuật

1. `host_keys::init` lúc app setup.
2. `ClientHandler::check_server_key` (russh) → `host_keys::verify`.
3. Chưa có entry → lưu mới (TOFU).
4. Có entry, khớp → OK.
5. Có entry, không khớp → `bail!` + emit status `error`.

Wildcard bastion: verify theo **IP resolved**, không theo pattern `10.0.*.1`.

## IPC

| Command | Mô tả |
|---|---|
| `list_host_keys` | Map tất cả fingerprints |
| `forget_host_key(host, port, tunnelId?)` | Xóa key; nếu có `tunnelId` cũng xóa IP resolved đang cache |

## Source files

| File | Vai trò |
|---|---|
| `host_keys.rs` | Store, verify, forget, persist atomic |
| `tunnel.rs` | `ClientHandler` hook |
| `commands.rs` | IPC forget/list |
| `lib.rs` | `host_keys::init` |

## Gotchas

- **Reinstall bastion** — fingerprint đổi → user phải Forget host key.
- **Wildcard + IP đổi** — key lưu theo IP cũ; IP mới = TOFU mới hoặc mismatch nếu trùng IP khác server.
- **Không có UI list đầy đủ** — chỉ Forget per tunnel; `list_host_keys` có sẵn cho tương lai.
- Lỗi hiển thị gợi ý: *"If the server was reinstalled, forget the stored key and reconnect."*
