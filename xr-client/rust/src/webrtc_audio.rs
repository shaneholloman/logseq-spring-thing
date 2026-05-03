//! Spatial voice routing surface. The livekit-android AAR integration is a
//! follow-up (PRD-008 §5.5). This module exposes the API that scene-side code
//! and other gdext modules will call against, so wiring lands first; the
//! adapter that talks to the JNI bridge slots in behind these methods.

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;
#[cfg(not(test))]
use tracing::info;

#[cfg(not(test))]
use godot::prelude::*;

#[derive(Debug, Clone, Error)]
pub enum VoiceError {
    #[error("track for did {did} not attached")]
    UnknownTrack { did: String },
    #[error("livekit binding not yet wired (PRD-008 §5.5)")]
    NotImplemented,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListenerTransform {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceTrackState {
    pub did: String,
    pub position: [f32; 3],
    pub muted: bool,
}

pub struct SpatialVoiceRouterCore {
    tracks: Mutex<HashMap<String, VoiceTrackState>>,
    listener: Mutex<ListenerTransform>,
}

impl SpatialVoiceRouterCore {
    pub fn new() -> Self {
        Self {
            tracks: Mutex::new(HashMap::new()),
            listener: Mutex::new(ListenerTransform {
                position: [0.0; 3],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
            }),
        }
    }

    pub fn attach_track(&self, did: String, position: [f32; 3]) -> Result<(), VoiceError> {
        let mut tracks = self
            .tracks
            .lock()
            .map_err(|_| VoiceError::NotImplemented)?;
        tracks.insert(
            did.clone(),
            VoiceTrackState {
                did,
                position,
                muted: false,
            },
        );
        Ok(())
    }

    pub fn detach_track(&self, did: &str) -> Result<(), VoiceError> {
        let mut tracks = self
            .tracks
            .lock()
            .map_err(|_| VoiceError::NotImplemented)?;
        tracks
            .remove(did)
            .ok_or_else(|| VoiceError::UnknownTrack {
                did: did.to_owned(),
            })?;
        Ok(())
    }

    pub fn update_track_position(&self, did: &str, position: [f32; 3]) -> Result<(), VoiceError> {
        let mut tracks = self
            .tracks
            .lock()
            .map_err(|_| VoiceError::NotImplemented)?;
        let track = tracks
            .get_mut(did)
            .ok_or_else(|| VoiceError::UnknownTrack {
                did: did.to_owned(),
            })?;
        track.position = position;
        Ok(())
    }

    pub fn set_track_muted(&self, did: &str, muted: bool) -> Result<(), VoiceError> {
        let mut tracks = self
            .tracks
            .lock()
            .map_err(|_| VoiceError::NotImplemented)?;
        let track = tracks
            .get_mut(did)
            .ok_or_else(|| VoiceError::UnknownTrack {
                did: did.to_owned(),
            })?;
        track.muted = muted;
        Ok(())
    }

    pub fn update_listener(&self, t: ListenerTransform) -> Result<(), VoiceError> {
        let mut listener = self
            .listener
            .lock()
            .map_err(|_| VoiceError::NotImplemented)?;
        *listener = t;
        Ok(())
    }

    pub fn track_count(&self) -> usize {
        self.tracks.lock().map(|t| t.len()).unwrap_or(0)
    }

    pub fn snapshot(&self) -> Vec<VoiceTrackState> {
        match self.tracks.lock() {
            Ok(t) => t.values().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for SpatialVoiceRouterCore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(test))]
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct SpatialVoiceRouter {
    core: SpatialVoiceRouterCore,
    base: Base<RefCounted>,
}

#[cfg(not(test))]
#[godot_api]
impl SpatialVoiceRouter {
    #[func]
    fn create() -> Gd<Self> {
        Gd::from_init_fn(|base| Self {
            core: SpatialVoiceRouterCore::new(),
            base,
        })
    }

    #[func]
    fn attach_track(&mut self, did: GString, position: Vector3) -> bool {
        match self
            .core
            .attach_track(did.to_string(), [position.x, position.y, position.z])
        {
            Ok(()) => {
                info!(did = %did, "voice track attached (stub)");
                true
            }
            Err(_) => false,
        }
    }

    #[func]
    fn detach_track(&mut self, did: GString) -> bool {
        self.core.detach_track(&did.to_string()).is_ok()
    }

    #[func]
    fn update_listener(&mut self, position: Vector3, forward: Vector3, up: Vector3) -> bool {
        self.core
            .update_listener(ListenerTransform {
                position: [position.x, position.y, position.z],
                forward: [forward.x, forward.y, forward.z],
                up: [up.x, up.y, up.z],
            })
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_increments_count() {
        let r = SpatialVoiceRouterCore::new();
        assert_eq!(r.track_count(), 0);
        r.attach_track("did:nostr:aaa".into(), [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(r.track_count(), 1);
    }

    #[test]
    fn detach_removes_track() {
        let r = SpatialVoiceRouterCore::new();
        r.attach_track("did:nostr:aaa".into(), [0.0; 3]).unwrap();
        r.detach_track("did:nostr:aaa").unwrap();
        assert_eq!(r.track_count(), 0);
    }

    #[test]
    fn detach_unknown_errors() {
        let r = SpatialVoiceRouterCore::new();
        let err = r.detach_track("did:nostr:nope").unwrap_err();
        assert!(matches!(err, VoiceError::UnknownTrack { .. }));
    }

    #[test]
    fn update_position_changes_snapshot() {
        let r = SpatialVoiceRouterCore::new();
        r.attach_track("did:nostr:aaa".into(), [0.0; 3]).unwrap();
        r.update_track_position("did:nostr:aaa", [9.0, 8.0, 7.0]).unwrap();
        let snap = r.snapshot();
        assert_eq!(snap[0].position, [9.0, 8.0, 7.0]);
    }

    #[test]
    fn mute_toggle_persists() {
        let r = SpatialVoiceRouterCore::new();
        r.attach_track("did:nostr:aaa".into(), [0.0; 3]).unwrap();
        r.set_track_muted("did:nostr:aaa", true).unwrap();
        assert!(r.snapshot()[0].muted);
    }
}
