use std::sync::Arc;

use visionclaw_xr_gdext::ports::fakes::{FakeSigner, FakeWsTransport};
use visionclaw_xr_gdext::ports::WsMessage;
use visionclaw_xr_gdext::presence::{PresenceClient, PresenceRoomState, RemoteMember};
use visionclaw_xr_presence::wire::{decode, OPCODE_AVATAR_POSE};
use visionclaw_xr_presence::{PoseFrame, RoomId, Transform};

fn room() -> RoomId {
    RoomId::parse("urn:visionclaw:room:sha256-12-aaaaaaaaaaaa").unwrap()
}

#[tokio::test]
async fn handshake_and_pose_round_trip_through_fake_transport() {
    let (transport, inbox) = FakeWsTransport::new();
    let transport = Arc::new(transport);
    let signer = Arc::new(FakeSigner::new());
    let mut client = PresenceClient::new(transport.clone(), signer.clone(), room());

    let server_state = PresenceRoomState {
        room_id: room().as_str().to_owned(),
        members: vec![RemoteMember {
            did: format!("did:nostr:{}", "c".repeat(64)),
            display_name: "carol".into(),
            avatar_id: format!("urn:visionclaw:avatar:{}", "c".repeat(64)),
            model_uri: None,
        }],
    };
    inbox
        .send(WsMessage::Text(serde_json::to_string(&server_state).unwrap()))
        .await
        .unwrap();

    let state = client.handshake("alice".into()).await.unwrap();
    assert_eq!(state.members.len(), 1);

    let frame = PoseFrame {
        timestamp_us: 99_000,
        head: Transform {
            position: [0.5, 1.7, -0.2],
            rotation: [0.0, 0.0, 0.0, 1.0],
        },
        left_hand: Some(Transform::identity()),
        right_hand: Some(Transform::identity()),
    };
    client.send_pose(&frame).await.unwrap();

    let sent = transport.sent_binary.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0][0], OPCODE_AVATAR_POSE);

    let decoded = decode(&sent[0]).unwrap();
    assert_eq!(decoded.frame, frame);
}

#[tokio::test]
async fn server_close_during_init_yields_rejected() {
    let (transport, inbox) = FakeWsTransport::new();
    let transport = Arc::new(transport);
    let signer = Arc::new(FakeSigner::new());
    let mut client = PresenceClient::new(transport, signer, room());

    inbox.send(WsMessage::Close).await.unwrap();

    let err = client.handshake("alice".into()).await.unwrap_err();
    assert!(matches!(
        err,
        visionclaw_xr_gdext::presence::PresenceError::Rejected(_)
    ));
}

#[tokio::test]
async fn send_pose_before_handshake_is_rejected() {
    let (transport, _inbox) = FakeWsTransport::new();
    let client = PresenceClient::new(
        Arc::new(transport),
        Arc::new(FakeSigner::new()),
        room(),
    );
    let frame = PoseFrame {
        timestamp_us: 1,
        head: Transform::identity(),
        left_hand: None,
        right_hand: None,
    };
    let err = client.send_pose(&frame).await.unwrap_err();
    assert!(matches!(
        err,
        visionclaw_xr_gdext::presence::PresenceError::Protocol(_)
    ));
}

#[tokio::test]
async fn handshake_with_empty_members_succeeds() {
    let (transport, inbox) = FakeWsTransport::new();
    let transport = Arc::new(transport);
    let signer = Arc::new(FakeSigner::new());
    let mut client = PresenceClient::new(transport.clone(), signer, room());

    let server_state = PresenceRoomState {
        room_id: room().as_str().to_owned(),
        members: vec![],
    };
    inbox
        .send(WsMessage::Text(serde_json::to_string(&server_state).unwrap()))
        .await
        .unwrap();

    let state = client.handshake("alice".into()).await.unwrap();
    assert!(state.members.is_empty());
    assert!(client.avatar_id().is_some());
}
