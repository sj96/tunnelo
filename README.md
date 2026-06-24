# Tunnelo

Ứng dụng desktop (Tauri + React) giúp truy cập dịch vụ nội bộ qua **SSH bastion** — như mở cổng local tới `gitlab.example.com` mà không cần VPN hay sửa DNS công khai.

A desktop app (Tauri + React) that reaches internal services through an **SSH bastion** — open `https://gitlab.example.com` in your browser as if you were on the network.

## Cách hoạt động / How it works

Mỗi tunnel mở **một phiên SSH** tới bastion, rồi tạo các **local forward (`-L`)** tới dịch vụ phía sau bastion.

```
Trình duyệt → 127.0.0.1 (hosts + router cục bộ)
            → cổng nội bộ trên máy bạn
            → SSH -L qua bastion
            → dịch vụ đích (vd. gitlab.example.com:443)
```

- **Hosts file**: ghi `127.0.0.1 <hostname>` (có marker `# tunnelo-managed`) để hostname trỏ về máy bạn.
- **Router cục bộ**: HTTPS qua SNI proxy `:443`; HTTP qua router theo cổng.
- **Bastion wildcard** (tuỳ chọn): nhập dạng `10.0.*.1` — app quét và chọn IP có SSH mở.

## Nguyên tắc / Principles

1. **Một SSH, nhiều forward** — một tunnel = một kết nối bastion; thêm/xoá mapping không cần SSH mới.
2. **Chỉ đụng hosts có marker** — app chỉ ghi/xoá dòng `# tunnelo-managed`; không sửa tay các dòng khác.
3. **Bí mật ở keyring OS** — mật khẩu SSH và passphrase key lưu trong keychain Windows, không nằm trong file cấu hình.
4. **Tin host key lần đầu (TOFU)** — lần đầu kết nối bastion mới sẽ lưu fingerprint; đổi server thì dùng **Forget host key** trên thẻ tunnel.
5. **Routing cục bộ, không cần server công khai** — không cài Caddy, không mở cổng public; chỉ cần SSH tới bastion.
6. **Chạy nền qua tray** — đóng cửa sổ vẫn giữ tunnel; click icon tray để mở lại.
7. **Tự kết nối lại** — mất mạng thì thử reconnect (có thể tắt trên từng tunnel).

## Yêu cầu / Prerequisites

- Máy **Windows** (client hiện tại).
- **SSH bastion** có thể truy cập (IP hoặc pattern wildcard).
- Quyền ghi **hosts file** — Windows có thể hỏi UAC khi cần.
- Key SSH, mật khẩu, hoặc ssh-agent.

## Sử dụng nhanh / Quick start

1. Cài hoặc chạy dev (xem bên dưới).
2. **+ New tunnel** → điền bastion (host, port, user, auth), thêm ít nhất một forward (URL đích, vd. `gitlab.example.com:443`).
3. **Start** — khi Connected, mở URL trên thẻ tunnel (hoặc Copy).
4. Bật **Auto-start on launch** nếu muốn tunnel tự chạy khi mở app.

Lỗi host key sau khi cài lại server → **Forget host key** trên menu thẻ tunnel.

## Cài đặt & chạy / Install & run

**Phát triển / Development**

```bash
pnpm install
pnpm tauri dev
```

**Build installer Windows (NSIS)**

```bash
pnpm tauri build
# → src-tauri/target/release/bundle/nsis/Tunnelo_<version>_x64-setup.exe
```

Lệnh khác:

```bash
pnpm build                    # build frontend
cd src-tauri && cargo check   # kiểm tra Rust backend
```

## Cấu trúc dự án / Project layout

```
src/                     React UI (TunnelCard, TunnelForm, ConsolePanel)
src-tauri/src/
  tunnel.rs              SSH engine (-L forwards, reconnect)
  local_router.rs        hosts + SNI/HTTP routing
  hosts.rs, elevation.rs hosts file (UAC trên Windows)
  bastion_resolve.rs     quét bastion wildcard
  secrets.rs, host_keys.rs  keyring + TOFU
  commands.rs, store.rs  IPC + lưu profile JSON
```
