# OpenCordUIKit Swift Package

This package provides the iOS client-side foundation for OpenCord:

- `OpenCordClientCore`: shared DTOs, errors, realtime envelope decoding, and client protocols.
- `OpenCordNetworking`: HTTP `/v1` API client + websocket `/v1/stream` realtime client.
- `OpenCordUIKitShell`: UIKit view-controller shell for auth, guilds, timeline, and voice controls.

Runnable app target:
- `../OpenCordUIKitApp` (XcodeGen project that embeds this package)

## Local Validation

```bash
cd ios/OpenCordUIKit
swift test
```

## Service Contract

`OpenCordNetworking` targets the backend service routes introduced in `discord_service`:

- `POST /v1/auth/discord/start`
- `POST /v1/auth/logout`
- `GET /v1/me`
- `GET /v1/guilds`
- `GET /v1/guilds/{guild_id}/channels`
- `GET/POST /v1/channels/{channel_id}/messages`
- `PATCH/DELETE /v1/channels/{channel_id}/messages/{message_id}`
- `POST /v1/voice/sessions`
- `POST /v1/voice/sessions/{voice_session_id}/speaking`
- `DELETE /v1/voice/sessions/{voice_session_id}`
- `GET /v1/stream` (websocket)
