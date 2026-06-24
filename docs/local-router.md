# Local Router

## User-facing

User chỉ nhập **hostname đích** (vd. `gitlab.example.com:443`). App tự:

- Ghi hosts → `127.0.0.1 gitlab.example.com`
- Mở router trên `:443` (HTTPS) hoặc `:80`/cổng khác (HTTP)
- Chuyển traffic tới cổng nội bộ (49100–49999) mà SSH `-L` đang listen

User mở `https://gitlab.example.com` trong trình duyệt như bình thường.

## Luồng kỹ thuật

```
Browser → 127.0.0.1:443
       → SniProxy peek TLS ClientHello → parse SNI hostname
       → resolve(hostname) → internal_port (49100+)
       → TCP proxy tới 127.0.0.1:internal_port
       → SSH -L forward → remote service
```

HTTP (port ≠ 443):

```
Browser → 127.0.0.1:<port>
       → HttpRouter peek request → parse Host header
       → resolve → internal_port → SSH -L
```

## Port pool

- Range: **49100–49999**
- Mỗi cặp `(hostname, public_port)` được một `internal_port`.
- Ref-count: nhiều tunnel cùng hostname chia sẻ route; giảm ref khi stop.

## Route key

```rust
RouteKey { hostname, public_port }  // public_port = remotePort (443, 80, …)
```

## Router lifecycle

- **SNI proxy** (`127.0.0.1:443`): start khi có route port 443; stop khi không còn.
- **HTTP router** (`127.0.0.1:<port>`): một listener mỗi cổng HTTP đang dùng.
- `shutdown_all` (Quit từ tray): dọn routes, routers, hosts.

## IP đích

Nếu `remoteHost` parse được là IP → **bỏ qua router**, bind thẳng `127.0.0.1:remotePort`.

## Source files

| File | Vai trò |
|---|---|
| `local_router.rs` | Orchestrator, port pool, activate/deactivate |
| `sni_proxy.rs` | TLS SNI passthrough `:443` |
| `sni_tls.rs` | Parse SNI từ ClientHello |
| `http_router.rs` | HTTP Host-header passthrough |
| `hosts.rs` | Sync domains sau activate/deactivate |

## Gotchas

- **Port 443 bận** — lỗi `"starting SNI router on 127.0.0.1:443 (is another service using port 443?)"`. Tắt IIS/skynet/etc. hoặc dịch vụ khác bind `:443`.
- **Domain conflict** — hai mapping khác `remotePort` cho cùng hostname → lỗi conflict.
- **Pool cạn** — tối đa ~900 port; lỗi `"internal port pool exhausted"`.
- **SNI không parse được** — fallback hostname `localhost` (thường không route được).
- Activate hai lần cùng tunnel id → `"tunnel routing already active"`.
