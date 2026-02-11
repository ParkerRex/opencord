# OpenCordUIKitApp

`OpenCordUIKitApp` is a runnable iOS UIKit shell target that consumes the
`ios/OpenCordUIKit` package products.

## Generate Project

```bash
cd ios/OpenCordUIKitApp
xcodegen generate
```

This produces `OpenCordUIKitApp.xcodeproj` with:
- OAuth-capable auth flow (`/v1/auth/discord/start` + callback deep-link)
- Persistent session storage via `UserDefaultsAuthSessionStore`
- Guild/channel/message timeline powered by `OpenCordNetworking`

Default service base URL is `http://127.0.0.1:8080`.
