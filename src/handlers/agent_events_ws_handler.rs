//! WebSocket handler for ADR-059 / ADR-014 bidirectional agent channel.
//!
//! Endpoint: `/wss/agent-events`
//! Subprotocol: `vc-agent-events.v1`
//!
//! Inbound: JSON `AgentActionEnvelope` from agentbox WS subscribers.
//!   On receipt, spawns a transient `BeamEdge` + `ChargeModulation` for
//!   `duration_ms` via `TransientEdgeActor`.
//! Outbound: `UserInteractionEvent` broadcast to all connected agentbox
//!   subscribers (Phase 3 — `BroadcastUserInteraction` message).

use crate::agent_events::transient::{BeamEdge, ChargeModulation};
use crate::agent_events::{AgentActionEnvelope, UserInteractionEvent, WS_SUBPROTOCOL};
use crate::actors::transient_edge_actor::{SpawnBeam, TransientEdgeActor};
use actix::{Actor, ActorContext, Addr, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use log::{debug, info, warn};
use std::sync::{Arc, RwLock};

/// Process-wide registry of connected agent-events sockets.
/// Phase 3 broadcasts `user_interaction` events to all entries.
pub type AgentEventsBroadcaster = Arc<RwLock<Vec<Addr<AgentEventsSession>>>>;

pub fn new_broadcaster() -> AgentEventsBroadcaster {
    Arc::new(RwLock::new(Vec::new()))
}

/// One per WS connection.
pub struct AgentEventsSession {
    transient_addr: Addr<TransientEdgeActor>,
    broadcaster: AgentEventsBroadcaster,
}

impl AgentEventsSession {
    pub fn new(
        transient_addr: Addr<TransientEdgeActor>,
        broadcaster: AgentEventsBroadcaster,
    ) -> Self {
        Self {
            transient_addr,
            broadcaster,
        }
    }

    fn handle_inbound_text(&self, text: &str) {
        // Discriminate by JSON `type` field. Two known values today:
        // `agent_action` (inbound from agentbox) and `user_interaction`
        // (we expect this only outbound, but accept echo per ADR-059
        // recommendation #1 behind dev-only header — TODO Phase 3).
        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                warn!("[agent-events] malformed JSON frame: {}", e);
                return;
            }
        };
        let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match event_type {
            "agent_action" => self.handle_agent_action(value),
            "user_interaction" => {
                // Inbound echo — accepted for testing, dev-only.
                debug!("[agent-events] received user_interaction echo (dev path)");
            }
            other => warn!("[agent-events] unknown event type: {}", other),
        }
    }

    fn handle_agent_action(&self, value: serde_json::Value) {
        let envelope: AgentActionEnvelope = match serde_json::from_value(value) {
            Ok(e) => e,
            Err(e) => {
                warn!("[agent-events] agent_action parse error: {}", e);
                return;
            }
        };
        let color = envelope.beam_color().to_string();
        let beam = BeamEdge::new(
            envelope.source_agent_id,
            envelope.target_node_id,
            envelope.action_type,
            color,
            envelope.duration_ms as u32,
        );
        let modulation = ChargeModulation::new(
            envelope.source_agent_id,
            envelope.target_node_id,
            envelope.duration_ms as u32,
        );
        self.transient_addr.do_send(SpawnBeam { beam, modulation });
    }
}

impl Actor for AgentEventsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Register this session with the broadcaster for outbound user_interaction.
        if let Ok(mut guard) = self.broadcaster.write() {
            guard.push(ctx.address());
        }
        info!("[agent-events] session started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        // De-register on disconnect. We can't easily compare Addr equality,
        // so we drop any closed addresses on next broadcast pass instead.
        let _ = ctx; // intentional
        info!("[agent-events] session stopped");
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AgentEventsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => self.handle_inbound_text(&text),
            Ok(ws::Message::Binary(_)) => {
                // Phase 1: binary frame parity (0x23) deferred. Currently ignored.
                debug!("[agent-events] binary frame (ignored, JSON-only in Phase 2)");
            }
            Ok(ws::Message::Ping(p)) => ctx.pong(&p),
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Close(reason)) => {
                info!("[agent-events] client closed: {:?}", reason);
                ctx.stop();
            }
            Ok(ws::Message::Continuation(_)) | Ok(ws::Message::Nop) => {}
            Err(e) => {
                warn!("[agent-events] ws error: {}", e);
                ctx.stop();
            }
        }
    }
}

/// Outbound message: broadcast a `UserInteractionEvent` to this session.
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastUserInteraction(pub UserInteractionEvent);

impl Handler<BroadcastUserInteraction> for AgentEventsSession {
    type Result = ();

    fn handle(&mut self, msg: BroadcastUserInteraction, ctx: &mut Self::Context) {
        if let Ok(json) = serde_json::to_string(&msg.0) {
            ctx.text(json);
        }
    }
}

/// Handler entry point — mounted at `/wss/agent-events` in `main.rs`.
pub async fn agent_events_handler(
    req: HttpRequest,
    stream: web::Payload,
    transient: web::Data<Addr<TransientEdgeActor>>,
    broadcaster: web::Data<AgentEventsBroadcaster>,
) -> Result<HttpResponse, actix_web::Error> {
    // Subprotocol negotiation per ADR-059 §1.
    let requested = req
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let accepts = requested
        .split(',')
        .map(|s| s.trim())
        .any(|p| p == WS_SUBPROTOCOL);

    let session = AgentEventsSession::new(transient.get_ref().clone(), broadcaster.get_ref().clone());

    let mut response = ws::start(session, &req, stream)?;
    if accepts {
        response.headers_mut().insert(
            actix_web::http::header::SEC_WEBSOCKET_PROTOCOL,
            WS_SUBPROTOCOL.parse().unwrap(),
        );
    }
    Ok(response)
}

/// Broadcast a `UserInteractionEvent` to every live agent-events session.
/// Removes dropped/dead addresses opportunistically.
pub fn broadcast_user_interaction(broadcaster: &AgentEventsBroadcaster, evt: UserInteractionEvent) {
    let mut to_keep = Vec::new();
    if let Ok(guard) = broadcaster.read() {
        for addr in guard.iter() {
            if addr.connected() {
                addr.do_send(BroadcastUserInteraction(evt.clone()));
                to_keep.push(addr.clone());
            }
        }
    }
    if let Ok(mut guard) = broadcaster.write() {
        *guard = to_keep;
    }
}
