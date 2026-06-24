# Tunnelo — Tài liệu kỹ thuật

Tài liệu nội bộ mô tả kiến trúc và từng module của Tunnelo (desktop app Tauri + React).

## Mục lục

| Tài liệu | Nội dung |
|---|---|
| [kien-truc.md](./kien-truc.md) | Tổng quan kiến trúc, luồng dữ liệu |
| [ssh-tunnel.md](./ssh-tunnel.md) | SSH engine, local forward (`-L`), reconnect |
| [local-router.md](./local-router.md) | Router cục bộ, SNI/HTTP, port pool |
| [hosts-file.md](./hosts-file.md) | Quản lý hosts file tự động |
| [bastion-wildcard.md](./bastion-wildcard.md) | Quét bastion wildcard IPv4 |
| [secrets.md](./secrets.md) | Keyring OS (mật khẩu, passphrase) |
| [host-keys.md](./host-keys.md) | TOFU — xác minh SSH host key |
| [profiles-store.md](./profiles-store.md) | Lưu profile JSON, migration |
| [ipc-events.md](./ipc-events.md) | Tauri commands + events |
| [platform-tray.md](./platform-tray.md) | Tray, đóng cửa sổ, platform paths |
| [giao-dien.md](./giao-dien.md) | React UI, components |
| [prd/README.md](./prd/README.md) | Product Requirements Document |

## Đọc nhanh

```
Trình duyệt → hosts (127.0.0.1) → router cục bộ (:443 SNI / :80 HTTP)
            → cổng nội bộ 49100–49999
            → SSH -L qua bastion
            → dịch vụ đích
```

Xem [README gốc](../README.md) cho hướng dẫn cài đặt và sử dụng.
