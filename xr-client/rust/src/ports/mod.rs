//! Hexagonal port traits for the gdext crate.
//!
//! All transport, identity, and Godot-side surfaces are reached through these
//! traits so unit tests can swap fakes without dragging in tokio sockets, the
//! Nostr signer, or a Godot runtime. Per `ddd-xr-godot-context.md` §2.4 ACL is
//! mandatory and named.

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

use visionclaw_xr_presence::ports::SignedChallenge;
use visionclaw_xr_presence::Did;

#[derive(Debug, Clone, Error)]
pub enum TransportError {
    #[error("websocket connect failed: {0}")]
    Connect(String),
    #[error("websocket send failed: {0}")]
    Send(String),
    #[error("websocket receive failed: {0}")]
    Recv(String),
    #[error("websocket closed")]
    Closed,
}

#[derive(Debug, Clone, Error)]
pub enum SignerError {
    #[error("signing failed: {0}")]
    Sign(String),
    #[error("signer unavailable")]
    Unavailable,
}

#[async_trait]
pub trait WsTransport: Send + Sync {
    async fn send_binary(&self, payload: Bytes) -> Result<(), TransportError>;
    async fn send_text(&self, payload: String) -> Result<(), TransportError>;
    async fn recv(&self) -> Result<WsMessage, TransportError>;
    async fn close(&self) -> Result<(), TransportError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMessage {
    Binary(Vec<u8>),
    Text(String),
    Close,
}

pub trait Signer: Send + Sync {
    fn pubkey_hex(&self) -> String;
    fn sign_challenge(
        &self,
        nonce: &[u8; 32],
        timestamp_us: u64,
    ) -> Result<SignedChallenge, SignerError>;
    fn did(&self) -> Result<Did, SignerError>;
}

pub mod fakes {
    use super::*;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::{mpsc, Mutex};

    pub struct FakeWsTransport {
        pub sent_binary: StdMutex<Vec<Vec<u8>>>,
        pub sent_text: StdMutex<Vec<String>>,
        inbox: Mutex<Option<mpsc::Receiver<WsMessage>>>,
    }

    impl FakeWsTransport {
        pub fn new() -> (Self, mpsc::Sender<WsMessage>) {
            let (tx, rx) = mpsc::channel(64);
            (
                Self {
                    sent_binary: StdMutex::new(Vec::new()),
                    sent_text: StdMutex::new(Vec::new()),
                    inbox: Mutex::new(Some(rx)),
                },
                tx,
            )
        }
    }

    #[async_trait]
    impl WsTransport for FakeWsTransport {
        async fn send_binary(&self, payload: Bytes) -> Result<(), TransportError> {
            self.sent_binary
                .lock()
                .map_err(|e| TransportError::Send(e.to_string()))?
                .push(payload.to_vec());
            Ok(())
        }
        async fn send_text(&self, payload: String) -> Result<(), TransportError> {
            self.sent_text
                .lock()
                .map_err(|e| TransportError::Send(e.to_string()))?
                .push(payload);
            Ok(())
        }
        async fn recv(&self) -> Result<WsMessage, TransportError> {
            let mut guard = self.inbox.lock().await;
            let rx = guard.as_mut().ok_or(TransportError::Closed)?;
            rx.recv().await.ok_or(TransportError::Closed)
        }
        async fn close(&self) -> Result<(), TransportError> {
            let mut guard = self.inbox.lock().await;
            guard.take();
            Ok(())
        }
    }

    pub struct FakeSigner {
        pub did: Did,
    }

    impl FakeSigner {
        pub fn new() -> Self {
            let did = Did::parse(format!("did:nostr:{}", "a".repeat(64)))
                .expect("FakeSigner DID parse");
            Self { did }
        }
    }

    impl Default for FakeSigner {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Signer for FakeSigner {
        fn pubkey_hex(&self) -> String {
            self.did.pubkey_hex().to_owned()
        }
        fn sign_challenge(
            &self,
            nonce: &[u8; 32],
            timestamp_us: u64,
        ) -> Result<SignedChallenge, SignerError> {
            Ok(SignedChallenge {
                nonce: *nonce,
                timestamp_us,
                claimed_pubkey_hex: self.pubkey_hex(),
                signature_hex: "00".repeat(64),
            })
        }
        fn did(&self) -> Result<Did, SignerError> {
            Ok(self.did.clone())
        }
    }
}
