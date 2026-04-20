//! Broker actor messages (ADR-041).
//!
//! These messages are the public mailbox contract of `BrokerActor`. All
//! payloads are owned (no borrows) so they cross thread boundaries safely.

use actix::prelude::*;

use crate::domain::broker::{BrokerCase, DecisionOutcome};

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Create a new case in the broker's inbox.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<BrokerCase, String>")]
pub struct SubmitBrokerCase {
    pub case: BrokerCase,
}

/// Record a decision against an existing case.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<String, String>")]
pub struct DecideBrokerCase {
    pub case_id: String,
    pub decision_id: String,
    pub outcome: DecisionOutcome,
    pub broker_pubkey: String,
    pub reasoning: String,
}

/// Broker claims a case for review.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<(), String>")]
pub struct ClaimBrokerCase {
    pub case_id: String,
    pub broker_pubkey: String,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// List cases currently cached in the actor's inbox.
#[derive(Message, Debug, Clone)]
#[rtype(result = "Vec<BrokerCase>")]
pub struct ListBrokerInbox {
    pub limit: usize,
}

/// Fetch a single case from the actor's inbox cache (None if not resident).
#[derive(Message, Debug, Clone)]
#[rtype(result = "Option<BrokerCase>")]
pub struct GetBrokerCase {
    pub case_id: String,
}

// ---------------------------------------------------------------------------
// Subscription — WebSocket
// ---------------------------------------------------------------------------

/// Register a subscriber that wants real-time inbox events.
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct SubscribeBrokerChannel {
    pub client_id: String,
    pub channel: BrokerChannel,
}

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct UnsubscribeBrokerChannel {
    pub client_id: String,
    pub channel: BrokerChannel,
}

/// Subscription scopes the client may request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BrokerChannel {
    /// All inbox events (new case, updated priority, claim/release).
    Inbox,
    /// Events for a specific case id.
    Case(String),
}
