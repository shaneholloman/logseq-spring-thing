//! VisionClaw Quest 3 native APK — gdext hot-path crate (PRD-008).
//!
//! Owns:
//! - 0x42 graph position frame decode (`binary_protocol`)
//! - 0x43 avatar pose presence client (`presence`) — wire format from
//!   `visionclaw_xr_presence::wire`
//! - hand-tracking ray cast + pinch detection (`interaction`)
//! - distance-bucket LOD policy (`lod`)
//! - spatial voice routing surface (`webrtc_audio`)
//!
//! GDScript drives scene composition only; this crate owns every byte that
//! crosses the wire and every threshold that gates a pose / hit / level.

pub mod binary_protocol;
pub mod interaction;
pub mod lod;
pub mod ports;
pub mod presence;
pub mod webrtc_audio;

#[cfg(not(test))]
use godot::prelude::*;

#[cfg(not(test))]
struct VisionclawXrExtension;

#[cfg(not(test))]
#[gdextension]
unsafe impl ExtensionLibrary for VisionclawXrExtension {}
