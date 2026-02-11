# Swift UIKit Integration Plan

## Integration Shape

- Keep Rust core as protocol/domain engine.
- Expose narrow FFI API surface for Swift.
- UIKit app owns UI state, navigation, and platform rendering.

## Bridge Layers

1. Rust public FFI layer (C ABI boundary).
2. Swift wrapper module converting C ABI to Swift async-friendly APIs.
3. Feature modules consume wrapper via protocols.

## Suggested Milestones

1. Bridge auth/session calls first.
2. Bridge read-only timeline/guild/channel fetches.
3. Add gateway event stream bridge.
4. Add write operations and media/voice hooks.

## Non-Goals for First Iteration

- Full parity with desktop-native module behavior.
- Recreating all desktop IPC semantics in iOS.
