# Bastion Wildcard

## User-facing

Thay vì IP cố định, user có thể nhập **pattern IPv4** với `*`:

```
10.0.*.1
192.168.1.*
```

App quét các IP trong pattern, tìm máy có **cổng SSH mở** (`ssh.port`, mặc định 22), rồi kết nối IP đầu tiên tìm được.

## Luồng kỹ thuật

1. `is_wildcard_host` — pattern có đúng 4 octet và ít nhất một `*`.
2. `expand_wildcard` — sinh danh sách `Ipv4Addr` (`*` = 1–254, không gồm .0/.255).
3. `scan_ssh_port` — TCP connect song song (32 concurrent, timeout 500ms), trả IP đầu tiên OK.
4. Cache IP resolved per tunnel id; reconnect dùng cache.
5. Connect fail với cache → **rescan** (log `"Cached X unreachable — rescanning"`).

Host key TOFU lưu theo **IP resolved**, không theo pattern. **Forget host key** cũng xóa key của IP resolved (nếu có).

## Giới hạn

| Giới hạn | Giá trị |
|---|---|
| Max candidates | **512** |
| Scan concurrency | 32 |
| Scan timeout | 500ms / IP |

Pattern quá lớn (vd. `*.*.*.*`, `192.168.*.*`) → lỗi validation.

## UI validation

`src/bastionHost.ts` mirror logic Rust — validate trước khi save (max 512 candidates).

## Source files

| File | Vai trò |
|---|---|
| `bastion_resolve.rs` | expand, scan, `is_wildcard_host` |
| `tunnel.rs` | `resolve_bastion_host`, cache `resolved_bastion` |
| `commands.rs` | `forget_host_key` xóa cả IP resolved |
| `src/bastionHost.ts` | Validation phía UI |

## Gotchas

- **Chỉ IPv4** — hostname DNS (vd. `bastion.corp.com`) không phải wildcard pattern.
- **IP đầu tiên thắng** — không ping/health check; chỉ TCP connect port SSH.
- **Nhiều bastion cùng pattern** — có thể chọn IP khác mỗi lần scan nếu thứ tự scan thay đổi.
- **Status UI** — `resolvedBastionHost` hiện trên thẻ tunnel khi connected.
- Log level `scan` trong console khi quét.
