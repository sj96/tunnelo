# SSH Tunnel (`-L`)

## User-facing

Mỗi tunnel = **một phiên SSH** tới bastion + **nhiều local forward** tới dịch vụ phía sau bastion. Bấm **Start** để mở; **Stop** để đóng listeners và ngắt SSH.

Tương đương CLI:

```bash
ssh -L <local>:<remoteHost>:<remotePort> ... user@bastion
```

## Luồng kỹ thuật

1. `TunnelManager::start` spawn async task `supervise`.
2. `LocalRouter::activate_tunnel` trả về `ActivatedMapping` (bind host/port thực tế).
3. `connect_session` → handshake + auth (`russh`).
4. Với mỗi mapping: `TcpListener::bind` trên client, mỗi connection → `channel_open_direct_tcpip(remoteHost, remotePort)`.
5. `copy_bidirectional` giữa socket local và SSH channel.
6. Tick 5s kiểm tra `session.is_closed()` → reconnect nếu `autoReconnect`.

## Auth

| Loại | Cách hoạt động |
|---|---|
| `agent` | OpenSSH agent (Windows: named pipe `\\.\pipe\openssh-ssh-agent`) |
| `key` | PEM trên disk; passphrase trong keyring |
| `password` | Password trong keyring lúc connect |

## Reconnect

- Backoff: 2s → 4s → … → **tối đa 30s**.
- Status: `reconnecting` giữa các lần thử.
- Tắt bằng `autoReconnect: false` trên profile.

## Keepalive

`keepalive_interval: 15s`, `keepalive_max: 3` trong `client::Config`.

## Mapping theo loại đích

| `remoteHost` | Bind local | Access URL |
|---|---|---|
| Hostname (vd. `gitlab.example.com`) | Port pool 49100–49999 + router | `https://hostname` |
| IP (vd. `10.0.1.5`) | Trực tiếp `127.0.0.1:remotePort` | `https://127.0.0.1` |

## Source files

- `src-tauri/src/tunnel.rs` — engine chính
- `src-tauri/src/model.rs` — `TunnelProfile`, `ForwardMapping`, `TunnelStatus`
- `src-tauri/src/commands.rs` — `start_tunnel`, `stop_tunnel`

## Gotchas

- **Tunnel đã chạy** — `start` trả lỗi `"tunnel already running"`.
- **Không có mapping** — lỗi `"no port forwards configured"`.
- **Host key mismatch** — status `error`, cần **Forget host key** (xem [host-keys.md](./host-keys.md)).
- **Stop** gửi shutdown signal; sau 8s task bị abort nếu chưa kết thúc.
- Wildcard bastion: xem [bastion-wildcard.md](./bastion-wildcard.md).
