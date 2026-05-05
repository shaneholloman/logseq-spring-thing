//! Presence WebSocket client: NIP-98 handshake, 0x43 pose frame round-trip,
//! avatar join/leave routing into Godot signals. Wire-level details live in
//! `visionclaw_xr_presence::wire`; this module owns lifecycle + signal fan-out.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;
#[cfg(not(test))]
use tracing::warn;

use visionclaw_xr_presence::wire::{decode, encode, DecodedFrame};
use visionclaw_xr_presence::{
    AvatarId, AvatarMetadata, Did, PoseFrame, RoomId, WireError,
};

use crate::ports::{Signer, SignerError, TransportError, WsMessage, WsTransport};

#[cfg(not(test))]
use godot::prelude::*;

#[derive(Debug, Clone, Error)]
pub enum PresenceError {
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("signer: {0}")]
    Signer(#[from] SignerError),
    #[error("wire: {0}")]
    Wire(#[from] WireError),
    #[error("init protocol failure: {0}")]
    Protocol(String),
    #[error("server rejected handshake: {0}")]
    Rejected(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceInit {
    pub room_id: String,
    pub display_name: String,
    pub did: String,
    pub timestamp_us: u64,
    pub nonce_hex: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceRoomState {
    pub room_id: String,
    pub members: Vec<RemoteMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMember {
    pub did: String,
    pub display_name: String,
    pub avatar_id: String,
    pub model_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PresenceControl {
    AvatarJoined { member: RemoteMember },
    AvatarLeft { avatar_id: String },
    Kick { reason: String },
}

pub struct PresenceClient<T: WsTransport, S: Signer> {
    transport: Arc<T>,
    signer: Arc<S>,
    room: RoomId,
    avatar: Option<AvatarId>,
}

impl<T: WsTransport, S: Signer> PresenceClient<T, S> {
    pub fn new(transport: Arc<T>, signer: Arc<S>, room: RoomId) -> Self {
        Self {
            transport,
            signer,
            room,
            avatar: None,
        }
    }

    pub async fn handshake(&mut self, display_name: String) -> Result<PresenceRoomState, PresenceError> {
        let did = self.signer.did()?;
        let nonce = [0u8; 32];
        let timestamp_us = current_micros();
        let challenge = self.signer.sign_challenge(&nonce, timestamp_us)?;
        let init = PresenceInit {
            room_id: self.room.as_str().to_owned(),
            display_name,
            did: did.to_string(),
            timestamp_us,
            nonce_hex: hex_lower(&challenge.nonce),
            signature_hex: challenge.signature_hex,
        };
        let init_json = serde_json::to_string(&init)
            .map_err(|e| PresenceError::Protocol(format!("encode init: {e}")))?;
        self.transport.send_text(init_json).await?;

        match self.transport.recv().await? {
            WsMessage::Text(t) => {
                let state: PresenceRoomState = serde_json::from_str(&t)
                    .map_err(|e| PresenceError::Protocol(format!("decode room state: {e}")))?;
                self.avatar = Some(AvatarId::from_did(&did));
                info!(room = %self.room, members = state.members.len(), "presence handshake ok");
                Ok(state)
            }
            WsMessage::Binary(_) => Err(PresenceError::Protocol(
                "expected text room state, got binary".into(),
            )),
            WsMessage::Close => Err(PresenceError::Rejected("server closed during init".into())),
        }
    }

    pub async fn send_pose(&self, frame: &PoseFrame) -> Result<(), PresenceError> {
        let avatar = self.avatar.as_ref().ok_or_else(|| {
            PresenceError::Protocol("send_pose before successful handshake".into())
        })?;
        let bytes = encode(frame, &self.room, avatar)?;
        self.transport.send_binary(bytes).await?;
        Ok(())
    }

    pub fn decode_pose(&self, bytes: &[u8]) -> Result<DecodedFrame, PresenceError> {
        Ok(decode(bytes)?)
    }

    pub fn avatar_id(&self) -> Option<&AvatarId> {
        self.avatar.as_ref()
    }
}

pub fn current_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn into_avatar_metadata(member: &RemoteMember) -> Result<AvatarMetadata, PresenceError> {
    let did = Did::parse(member.did.clone())
        .map_err(|e| PresenceError::Protocol(format!("bad DID in member: {e}")))?;
    Ok(AvatarMetadata {
        did,
        display_name: member.display_name.clone(),
        model_uri: member.model_uri.clone(),
    })
}

#[cfg(not(test))]
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct PresenceClientNode {
    base: Base<RefCounted>,
}

#[cfg(not(test))]
#[godot_api]
impl PresenceClientNode {
    #[signal]
    fn avatar_joined(did: GString, display_name: GString, avatar_id: GString);

    #[signal]
    fn avatar_left(avatar_id: GString);

    #[signal]
    fn avatar_pose_updated(
        avatar_id: GString,
        head_pos: Vector3,
        head_rot: Quaternion,
        has_left: bool,
        has_right: bool,
    );

    #[signal]
    fn presence_kicked(reason: GString);

    #[func]
    fn create() -> Gd<Self> {
        Gd::from_init_fn(|base| Self { base })
    }

    #[func]
    fn ingest_pose_bytes(&mut self, bytes: PackedByteArray) {
        match decode(bytes.as_slice()) {
            Ok(frame) => {
                let head = frame.frame.head;
                let q = head.rotation;
                let pos = Vector3::new(head.position[0], head.position[1], head.position[2]);
                self.base_mut().emit_signal(
                    "avatar_pose_updated",
                    &[
                        Variant::from(GString::from(frame.avatar_id.as_str())),
                        Variant::from(pos),
                        Variant::from(Quaternion::new(q[0], q[1], q[2], q[3])),
                        Variant::from(frame.frame.left_hand.is_some()),
                        Variant::from(frame.frame.right_hand.is_some()),
                    ],
                );
            }
            Err(e) => {
                warn!(err = %e, "presence pose decode failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::{FakeSigner, FakeWsTransport};
    use visionclaw_xr_presence::Transform;

    fn sample_room() -> RoomId {
        RoomId::parse("urn:visionclaw:room:sha256-12-deadbeefcafe").unwrap()
    }

    #[tokio::test]
    async fn handshake_round_trip() {
        let (transport, inbox) = FakeWsTransport::new();
        let transport = Arc::new(transport);
        let signer = Arc::new(FakeSigner::new());
        let mut client = PresenceClient::new(transport.clone(), signer, sample_room());

        let server_state = PresenceRoomState {
            room_id: sample_room().as_str().to_owned(),
            members: vec![RemoteMember {
                did: format!("did:nostr:{}", "b".repeat(64)),
                display_name: "bob".into(),
                avatar_id: format!("urn:visionclaw:avatar:{}", "b".repeat(64)),
                model_uri: None,
            }],
        };
        inbox
            .send(WsMessage::Text(serde_json::to_string(&server_state).unwrap()))
            .await
            .unwrap();

        let result = client.handshake("alice".into()).await.unwrap();
        assert_eq!(result.members.len(), 1);
        assert!(client.avatar_id().is_some());

        let sent = transport.sent_text.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let init: PresenceInit = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(init.display_name, "alice");
        assert_eq!(init.room_id, sample_room().as_str());
    }

    #[tokio::test]
    async fn send_pose_before_handshake_errors() {
        let (transport, _inbox) = FakeWsTransport::new();
        let client = PresenceClient::new(
            Arc::new(transport),
            Arc::new(FakeSigner::new()),
            sample_room(),
        );
        let frame = PoseFrame {
            timestamp_us: 1,
            head: Transform::identity(),
            left_hand: None,
            right_hand: None,
        };
        let err = client.send_pose(&frame).await.unwrap_err();
        assert!(matches!(err, PresenceError::Protocol(_)));
    }

    #[tokio::test]
    async fn send_pose_after_handshake_writes_0x43_frame() {
        let (transport, inbox) = FakeWsTransport::new();
        let transport = Arc::new(transport);
        let mut client = PresenceClient::new(
            transport.clone(),
            Arc::new(FakeSigner::new()),
            sample_room(),
        );
        let server_state = PresenceRoomState {
            room_id: sample_room().as_str().to_owned(),
            members: vec![],
        };
        inbox
            .send(WsMessage::Text(serde_json::to_string(&server_state).unwrap()))
            .await
            .unwrap();
        client.handshake("alice".into()).await.unwrap();

        let frame = PoseFrame {
            timestamp_us: 9000,
            head: Transform {
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
            left_hand: None,
            right_hand: None,
        };
        client.send_pose(&frame).await.unwrap();

        let sent = transport.sent_binary.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0][0], visionclaw_xr_presence::wire::OPCODE_AVATAR_POSE);
    }

    #[tokio::test]
    async fn pose_decode_round_trip_via_client() {
        let (transport, _inbox) = FakeWsTransport::new();
        let signer = Arc::new(FakeSigner::new());
        let client = PresenceClient::new(Arc::new(transport), signer.clone(), sample_room());
        let frame = PoseFrame {
            timestamp_us: 1234,
            head: Transform::identity(),
            left_hand: Some(Transform::identity()),
            right_hand: None,
        };
        let did = signer.did().unwrap();
        let avatar = AvatarId::from_did(&did);
        let bytes = encode(&frame, &sample_room(), &avatar).unwrap();
        let decoded = client.decode_pose(&bytes).unwrap();
        assert_eq!(decoded.frame, frame);
    }

    #[test]
    fn hex_lower_known_vector() {
        assert_eq!(hex_lower(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }

    #[test]
    fn into_avatar_metadata_valid() {
        let member = RemoteMember {
            did: format!("did:nostr:{}", "b".repeat(64)),
            display_name: "bob".into(),
            avatar_id: format!("urn:visionclaw:avatar:{}", "b".repeat(64)),
            model_uri: Some("https://example.com/model.glb".into()),
        };
        let meta = into_avatar_metadata(&member).unwrap();
        assert_eq!(meta.did.as_str(), &format!("did:nostr:{}", "b".repeat(64)));
        assert_eq!(meta.display_name, "bob");
        assert_eq!(meta.model_uri, Some("https://example.com/model.glb".into()));
    }

    #[test]
    fn into_avatar_metadata_bad_did() {
        let member = RemoteMember {
            did: "not-a-did".into(),
            display_name: "evil".into(),
            avatar_id: "urn:visionclaw:avatar:abc".into(),
            model_uri: None,
        };
        let err = into_avatar_metadata(&member).unwrap_err();
        assert!(matches!(err, PresenceError::Protocol(_)));
    }

    #[tokio::test]
    async fn handshake_binary_response_errors() {
        let (transport, inbox) = FakeWsTransport::new();
        let transport = Arc::new(transport);
        let signer = Arc::new(FakeSigner::new());
        let mut client = PresenceClient::new(transport, signer, sample_room());

        inbox
            .send(WsMessage::Binary(vec![0x42, 0x00]))
            .await
            .unwrap();

        let err = client.handshake("alice".into()).await.unwrap_err();
        assert!(matches!(err, PresenceError::Protocol(_)));
    }
}
