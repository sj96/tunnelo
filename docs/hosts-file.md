# Hosts File

## User-facing

App **tự ghi** `C:\Windows\System32\drivers\etc\hosts` (Windows) khi tunnel chạy — user **không cần sửa tay**.

Mỗi hostname trong mapping (không phải IP) được thêm:

```
127.0.0.1 gitlab.example.com # tunnelo-managed
```

Khi stop tunnel / quit app → dòng có marker được gỡ (nếu không còn tunnel nào dùng hostname đó).

## Marker

```rust
pub const MARKER: &str = "# tunnelo-managed";
```

App **chỉ** thêm/xóa dòng chứa marker. Dòng hosts khác (manual, corporate) **không bị đụng**.

## Luồng kỹ thuật

1. `LocalRouter::activate_tunnel` → tăng ref-count per domain.
2. `hosts::sync_domains` đọc hosts hiện tại, giữ dòng không có marker, thêm dòng managed mới.
3. `elevation::write_hosts_file` ghi file (UAC nếu cần trên Windows).
4. `deactivate_tunnel` / `shutdown_all` → sync lại, bỏ domain ref = 0.

## Bootstrap

`LocalRouter::bootstrap()` gọi `hosts::cleanup_orphans()` lúc app khởi động — xóa dòng `# tunnelo-managed` sót lại sau crash.

## Elevation (Windows)

Ghi hosts cần quyền admin. Flow:

1. Thử ghi trực tiếp.
2. Thất bại → UAC qua PowerShell 64-bit (WOW64-safe path `Sysnative` cho process 32-bit).

User hủy UAC → tunnel có thể lỗi routing setup.

## Source files

| File | Vai trò |
|---|---|
| `hosts.rs` | `sync_domains`, `cleanup_orphans`, `MARKER` |
| `elevation.rs` | `write_hosts_file`, UAC elevation |
| `local_router.rs` | Gọi sync khi activate/deactivate |

## Gotchas

- **UAC** — lần đầu hoặc khi không đủ quyền, Windows hiện prompt; cần Accept.
- **WOW64** — app 32-bit đọc/ghi đúng `System32\drivers\etc\hosts` qua `Sysnative`.
- **Không sửa marker tay** — app có thể ghi đè hoặc xóa dòng managed khi sync.
- **IP remoteHost** — không thêm hosts (truy cập qua `127.0.0.1` trực tiếp).
