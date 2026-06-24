# Giao diện (React UI)

## User-facing

UI chính: danh sách **tunnel cards**, form tạo/sửa, **console log** có thể resize. Mỗi card: Start/Stop, URL mở/copy, menu Forget host key, auto-start/reconnect toggles.

## Cấu trúc

```
src/
  App.tsx              Layout, state, event listeners
  api.ts               Tauri invoke wrappers
  types.ts             Types mirror Rust model
  bastionHost.ts       Wildcard validation
  urlUtils.ts          formatRemoteUrl
  components/
    TunnelCard.tsx     Thẻ tunnel + actions
    TunnelForm.tsx     Form tạo/sửa + secrets
    MappingRow.tsx     Một dòng forward (remoteHost:remotePort)
    ConsolePanel.tsx     Log theo tunnel
```

## State chính (`App.tsx`)

| State | Nguồn |
|---|---|
| `tunnels` | `list_tunnels` |
| `statuses` | `tunnel://state` + `tunnel_running` |
| `resolvedBastions` | `resolvedBastionHost` trong state event |
| `editing` | Form modal |
| `busy` | Lock UI lúc start/stop |

## Form tunnel

- **Bastion**: host (IP hoặc wildcard), port, user, auth.
- **Mappings**: `remoteHost` + `remotePort` — không nhập local bind (router lo).
- **Secrets**: `SecretInput` gọi `set_secret` sau save (password/passphrase).
- Validation wildcard: `isValidBastionHost` (`bastionHost.ts`).

## Tunnel card

- Hiện status badge: stopped / connecting / connected / reconnecting / error.
- URL từ `localUrls` khi connected (hoặc `profileLocalUrls` khi stopped).
- Wildcard: hiện resolved IP khi có.
- **Forget host key** → `forgetHostKey(ssh.host, ssh.port, tunnelId)`.

## Console

- Listen `tunnel://log`, filter theo tunnel đang chọn.
- Chiều cao lưu `localStorage` key `tunnelo.consoleHeight`.

## Source files

Xem cây `src/` ở trên. Backend model: `src-tauri/src/model.rs`.

## Gotchas

- Status `connected` lúc refresh nếu `tunnel_running` — có thể lệch brief lúc reconnecting.
- Toast hiện lỗi từ `tunnel://state` khi `error`.
- Legacy alias `publicUrls` / `publicUrl` vẫn emit từ Rust cho tương thích.
