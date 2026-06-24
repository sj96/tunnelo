# Secrets (Keyring)

## User-facing

Mật khẩu SSH và passphrase private key **không lưu trong file cấu hình**. Chúng nằm trong **OS keyring** (Windows Credential Manager).

Khi tạo/sửa tunnel chọn auth `password` hoặc `key` + passphrase → UI gọi `set_secret` lúc save.

## Key format

```
Service: tunnelo
Account: <tunnel-id>:<kind>
```

| `kind` | Dùng cho |
|---|---|
| `password` | SSH password auth |
| `passphrase` | Passphrase của private key |

## Luồng kỹ thuật

- **Save profile** — React gọi `set_secret` sau `save_tunnel` (nếu user nhập secret mới).
- **Connect** — `tunnel.rs::authenticate` gọi `secrets::get(id, kind)`.
- **Delete tunnel** — `delete_tunnel` gọi `secrets::delete_all(id)`.
- **UI check** — `has_secret` để hiện trạng thái "đã lưu" mà không đọc giá trị.

## Source files

| File | Vai trò |
|---|---|
| `secrets.rs` | `set`, `get`, `delete`, `has`, `delete_all` |
| `commands.rs` | IPC `set_secret`, `delete_secret`, `has_secret` |
| `tunnel.rs` | Đọc secret lúc auth |
| `src/api.ts` | Typed wrappers |
| `src/components/TunnelForm.tsx` | Nhập secret khi save |

## Gotchas

- **Thiếu secret** — connect lỗi `"no password stored"` / `"no key passphrase stored"`.
- **Đổi tunnel id** — secret gắn theo id; clone tunnel mới cần nhập lại.
- **Key path** — đường dẫn key nằm trong profile JSON; chỉ passphrase/password ở keyring.
- **Agent auth** — không dùng keyring; cần `ssh-agent` có key loaded.
