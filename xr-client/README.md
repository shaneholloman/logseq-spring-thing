# VisionClaw XR — Quest 3 Native APK

Godot 4.3 + godot-rust (gdext) + OpenXR client per
[PRD-008](../docs/PRD-008-xr-godot-replacement.md) and
[ADR-071](../docs/adr/ADR-071-xr-godot-replacement.md).

## Layout

```
xr-client/
├── project.godot                       Godot 4.3 project root
├── export_presets.cfg                  Quest 3 arm64 Android preset
├── visionclaw_xr_gdext.gdextension     Manifest binding the Rust .so
├── icon.svg                            Project icon
├── android-export-template-config.txt  OpenXR loader notes
├── permissions-required.md             Android permission justification
├── scenes/
│   ├── XRBoot.tscn                     Boot: OpenXR init + capability probe
│   ├── GraphScene.tscn                 Graph rendering + AvatarSpawner
│   ├── HUD.tscn                        Settings / room controls / debug
│   └── Avatar.tscn                     Per-remote-presence template
├── scripts/                            GDScript wiring (no business logic)
├── materials/                          gem / crystal_orb / agent_capsule
├── addons/                             Godot OpenXR Vendors (CI-installed)
└── rust/                               gdext crate (see ./rust/Cargo.toml)
```

## Build prerequisites

| Tool | Version | Notes |
|---|---|---|
| Godot 4.3 stable | 4.3 | Both editor and Android export templates |
| Godot OpenXR Vendors plugin | 3.0.x | Install via AssetLib at first project open |
| Rust toolchain | stable 1.82+ | `rust-toolchain.toml` not pinned in this dir; uses workspace default |
| Android target | `aarch64-linux-android` | `rustup target add aarch64-linux-android` |
| Android NDK | r26d | Pinned in `xr-client/android/local.properties.template` (created in W2) |
| `cargo-ndk` | latest | `cargo install cargo-ndk` |
| JDK | 17 | Android Gradle plugin requirement |

## Build steps (manual)

```bash
# 1. Build the gdext .so for the host OS (for editor preview)
cargo build -p visionclaw-xr-gdext --release

# 2. Build for Quest 3 (arm64 Android)
cd xr-client/rust
cargo ndk -t aarch64-linux-android -o ../addons/visionclaw_xr_gdext build --release
cd ../..

# 3. Open Godot, import project, install OpenXR Vendors via AssetLib

# 4. Export the APK
godot --headless --export-release "Quest 3 arm64" xr-client/export/visionclaw-xr.apk

# 5. Side-load to Quest 3 (developer mode + USB debugging)
adb install -r xr-client/export/visionclaw-xr.apk
adb shell am start -n uk.xrsystems.visionclaw.xr/com.godot.game.GodotApp
```

## Test the gdext crate (headless)

```bash
cargo test -p visionclaw-xr-gdext
```

48 tests across 4 integration files + per-module unit tests run in <1 s
on a workstation. No Quest, no Godot runtime, no network required.

## gdext-registered classes

Read by `scripts/graph_scene.gd`; the registration lives in
`rust/src/lib.rs`. Each class is a `RefCounted` constructed via
`ClassName.create()` from GDScript:

| Class | Module | Signals |
|---|---|---|
| `BinaryProtocolClient` | `binary_protocol.rs` | `position_updated(node_id: u32, position: Vector3, velocity: Vector3)` |
| `PresenceClientNode` | `presence.rs` | `avatar_joined(did, display_name, avatar_id)`, `avatar_left(avatar_id)`, `avatar_pose_updated(avatar_id, head_pos, head_rot, has_left, has_right)`, `presence_kicked(reason)` |
| `XrInteraction` | `interaction.rs` | `node_targeted(node_id, distance)`, `node_grabbed(node_id, position)`, `haptic_pulse(controller, intensity)` |
| `LodPolicy` | `lod.rs` | (no signals; `should_recompute()` + `classify_distance()` getters) |
| `SpatialVoiceRouter` | `webrtc_audio.rs` | (no signals; `attach_track`, `detach_track`, `update_listener`) |

## Cross-references

- **Wire format**: [`crates/visionclaw-xr-presence/src/wire.rs`](../crates/visionclaw-xr-presence/src/wire.rs) — opcode 0x43 single source of truth
- **Bounded context**: [`docs/ddd-xr-godot-context.md`](../docs/ddd-xr-godot-context.md) BC22
- **Threat model**: [`docs/xr-godot-threat-model.md`](../docs/xr-godot-threat-model.md)
- **Architecture**: [`docs/xr-godot-system-architecture.md`](../docs/xr-godot-system-architecture.md)
