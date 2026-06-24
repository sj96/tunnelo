# Platform & Tray

## System Tray

App chạy nền qua **system tray**:

- **Đóng cửa sổ (X)** → `hide`, không thoát (`WindowEvent::CloseRequested` + `prevent_close`).
- **Click trái icon tray** → hiện lại cửa sổ chính.
- **Menu tray**: **Show Tunnelo** | **Quit**.

Quit gọi `local_router.shutdown_all()` rồi `app.exit(0)` — dọn hosts, routers, forwards.

Tunnel **tiếp tục chạy** khi cửa sổ ẩn.

## Auto-start

Trong `lib.rs::setup`, mọi profile `autoStart: true` được `tunnels.start` ngay khi app mở (kể cả chỉ có tray).

## Platform helpers

`platform.rs`:

| Hàm | Mô tả |
|---|---|
| `default_ssh_key_path()` | `%USERPROFILE%\.ssh\id_ed25519` (Windows) |
| `resolve_key_path(path)` | Expand `~`, normalize separator |

IPC: `default_ssh_key_path` → UI pre-fill key path.

## Windows-specific

| Module | Chi tiết |
|---|---|
| `elevation.rs` | UAC cho hosts file |
| `hosts.rs` | WOW64 `Sysnative` path |
| `tunnel.rs` | Agent qua `\\.\pipe\openssh-ssh-agent` |

Client hiện tại target **Windows**; code có `#[cfg(not(windows))]` cho paths/agent Unix nhưng chưa ship UI installer.

## Source files

| File | Vai trò |
|---|---|
| `lib.rs` | Tray setup, window close, auto-start |
| `platform.rs` | Path helpers |
| `elevation.rs` | Privileged writes |

## Gotchas

- **Quit vs đóng cửa sổ** — chỉ Quit dừng tunnel; đóng cửa sổ không.
- **Tray không có Start/Stop** — điều khiển tunnel qua UI chính.
- **OpenSSH agent** trên Windows cần service chạy + `ssh-add` (xem log auth nếu lỗi).
