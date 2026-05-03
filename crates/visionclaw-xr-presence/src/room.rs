use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::delta::PoseDelta;
use crate::error::RoomError;
use crate::types::{AvatarId, AvatarMetadata, Did, PoseFrame, RoomId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvatarState {
    pub avatar_id: AvatarId,
    pub metadata: AvatarMetadata,
    pub last_frame: Option<PoseFrame>,
}

/// `PresenceRoom` aggregate root per `ddd-xr-godot-context.md` §3.4.
///
/// Enforces invariants:
/// - I-PR01: one DID one avatar per room
/// - I-AV01: per-avatar pose timestamps strictly monotonic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceRoom {
    id: RoomId,
    members: HashMap<AvatarId, AvatarState>,
    did_index: HashMap<Did, AvatarId>,
}

impl PresenceRoom {
    pub fn new(id: RoomId) -> Self {
        Self {
            id,
            members: HashMap::new(),
            did_index: HashMap::new(),
        }
    }

    pub fn id(&self) -> &RoomId {
        &self.id
    }

    pub fn join(
        &mut self,
        did: Did,
        metadata: AvatarMetadata,
    ) -> Result<AvatarId, RoomError> {
        if let Some(existing) = self.did_index.get(&did) {
            warn!(did = %did, existing = %existing, "duplicate join attempted");
            return Err(RoomError::DuplicateDid {
                did: did.to_string(),
                existing: existing.to_string(),
            });
        }
        let avatar_id = AvatarId::from_did(&did);
        self.did_index.insert(did, avatar_id.clone());
        self.members.insert(
            avatar_id.clone(),
            AvatarState {
                avatar_id: avatar_id.clone(),
                metadata,
                last_frame: None,
            },
        );
        debug!(avatar = %avatar_id, room = %self.id, "avatar joined");
        Ok(avatar_id)
    }

    pub fn leave(&mut self, avatar_id: &AvatarId) -> Result<(), RoomError> {
        let state = self
            .members
            .remove(avatar_id)
            .ok_or_else(|| RoomError::UnknownAvatar {
                avatar_id: avatar_id.to_string(),
            })?;
        self.did_index.remove(&state.metadata.did);
        debug!(avatar = %avatar_id, room = %self.id, "avatar left");
        Ok(())
    }

    /// Update an avatar's pose, returning the delta vs the previous frame.
    /// Caller is responsible for running pose validators before calling.
    pub fn update_pose(
        &mut self,
        avatar_id: &AvatarId,
        frame: PoseFrame,
    ) -> Result<PoseDelta, RoomError> {
        let state = self
            .members
            .get_mut(avatar_id)
            .ok_or_else(|| RoomError::UnknownAvatar {
                avatar_id: avatar_id.to_string(),
            })?;

        let delta = match &state.last_frame {
            Some(prev) => PoseDelta::between(prev, &frame),
            None => PoseDelta {
                timestamp_us: frame.timestamp_us,
                mask: crate::delta::TransformMask(0b111),
                head: Some(frame.head),
                left_hand: frame.left_hand,
                right_hand: frame.right_hand,
            },
        };
        state.last_frame = Some(frame);
        Ok(delta)
    }

    pub fn member(&self, avatar_id: &AvatarId) -> Option<&AvatarState> {
        self.members.get(avatar_id)
    }

    pub fn members(&self) -> impl Iterator<Item = &AvatarState> {
        self.members.values()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did(byte: u8) -> Did {
        Did::parse(format!("did:nostr:{}", format!("{:02x}", byte).repeat(32))).unwrap()
    }

    fn meta(d: &Did, name: &str) -> AvatarMetadata {
        AvatarMetadata {
            did: d.clone(),
            display_name: name.to_owned(),
            model_uri: None,
        }
    }

    #[test]
    fn join_then_leave() {
        let mut room = PresenceRoom::new(
            RoomId::parse("urn:visionclaw:room:sha256-12-aaaaaaaaaaaa").unwrap(),
        );
        let d = did(0x11);
        let id = room.join(d.clone(), meta(&d, "alice")).unwrap();
        assert_eq!(room.len(), 1);
        room.leave(&id).unwrap();
        assert!(room.is_empty());
    }

    #[test]
    fn duplicate_did_rejected() {
        let mut room = PresenceRoom::new(
            RoomId::parse("urn:visionclaw:room:sha256-12-bbbbbbbbbbbb").unwrap(),
        );
        let d = did(0x22);
        room.join(d.clone(), meta(&d, "bob")).unwrap();
        let err = room.join(d.clone(), meta(&d, "bob2")).unwrap_err();
        assert!(matches!(err, RoomError::DuplicateDid { .. }));
    }

    #[test]
    fn leave_unknown_avatar_rejected() {
        let mut room = PresenceRoom::new(
            RoomId::parse("urn:visionclaw:room:sha256-12-cccccccccccc").unwrap(),
        );
        let d = did(0x33);
        let id = AvatarId::from_did(&d);
        let err = room.leave(&id).unwrap_err();
        assert!(matches!(err, RoomError::UnknownAvatar { .. }));
    }
}
