//! DojoDiscoveryActor — periodic peer Type Index crawler (ADR-029 read side).
//!
//! Owns the read half of ADR-029: on a periodic tick it walks a list of peer
//! WebIDs, fetches each peer's `publicTypeIndex.jsonld`, and emits
//! `PeerRegistrationsDiscovered` events into the actor system. The
//! `SkillRegistrySupervisor` (agent C2) subscribes to these events and
//! materialises the `SkillIndex` read model.
//!
//! This actor is intentionally thin:
//!   - no domain knowledge of skills or profiles lives here;
//!   - the tick scheduler is a stub — `CrawlInterval` defaults to 5 minutes
//!     but real scheduling (jitter, peer prioritisation, staleness budget)
//!     will land with agent C2's supervisor wiring;
//!   - peer WebID sources are injected (`SetPeerSource`) so this actor can
//!     stand alone in unit tests without pulling in the contact graph.
//!
//! Supervised via `SupervisedActorTrait` using the default
//! `RestartWithBackoff` policy — a transient DNS/HTTP failure in a peer
//! crawl should not take down the parent.

use actix::prelude::*;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::actors::supervisor::SupervisedActorTrait;
use crate::services::pod_client::PodClient;
use crate::services::type_index_discovery::{
    discover_peer_registrations, TypeRegistration,
};

/// Default tick interval — intentionally slow; agent C2 will calibrate this
/// against the eventual peer-count and freshness SLA.
const DEFAULT_CRAWL_INTERVAL: Duration = Duration::from_secs(300);

/// Published when a crawl of a single peer has completed (successfully or
/// with an empty result). Subscribers (SkillRegistrySupervisor) decide what
/// to do with it. A dedicated event type keeps the wire contract stable as
/// scheduling changes.
#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub struct PeerRegistrationsDiscovered {
    pub peer_webid: String,
    pub class_filter: String,
    pub registrations: Vec<TypeRegistration>,
}

/// Recipient type alias so C2's SkillRegistrySupervisor can register itself
/// without this module depending on the supervisor's concrete type.
pub type DiscoverySubscriber = Recipient<PeerRegistrationsDiscovered>;

/// Inject or replace the peer WebID source. Current implementation is a
/// flat set; agent C2 will replace it with a query against the contact
/// graph. Intentionally Arc-wrapped so the caller can swap the source
/// without restarting this actor.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetPeerSource(pub Arc<dyn PeerSource>);

/// Add a single subscriber for `PeerRegistrationsDiscovered`.
#[derive(Message)]
#[rtype(result = "()")]
pub struct Subscribe(pub DiscoverySubscriber);

/// Trigger a one-shot crawl of the configured peer set — used by tests and
/// by the eventual MCP `dojo_refresh` hook owned by agent X1.
#[derive(Message)]
#[rtype(result = "()")]
pub struct CrawlNow;

/// Filter applied to peer registrations this actor surfaces. Default is
/// `urn:solid:AgentSkill`. Agents that want contributor profiles instead
/// instantiate a second actor with a different filter.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetClassFilter(pub String);

/// Peer source abstraction — returns the set of WebIDs to crawl. Real
/// impls query the contact graph; the default in-memory impl is used for
/// tests and for bootstrap before BC18 Contacts is wired.
pub trait PeerSource: Send + Sync {
    fn peers(&self) -> HashSet<String>;
}

/// Minimal static peer source for tests and bootstrap.
#[derive(Default, Clone)]
pub struct StaticPeerSource {
    pub peers: HashSet<String>,
}

impl PeerSource for StaticPeerSource {
    fn peers(&self) -> HashSet<String> {
        self.peers.clone()
    }
}

/// The actor itself.
pub struct DojoDiscoveryActor {
    client: PodClient,
    peer_source: Arc<dyn PeerSource>,
    subscribers: Vec<DiscoverySubscriber>,
    class_filter: String,
    interval: Duration,
}

impl DojoDiscoveryActor {
    /// Build a new discovery actor.
    ///
    /// `class_filter` selects which registrations to surface — typically
    /// `urn:solid:AgentSkill` for the C2 SkillRegistry path, but anything
    /// returned by `TypeIndexDocument::filter_by_class` is valid.
    pub fn new(client: PodClient, peer_source: Arc<dyn PeerSource>) -> Self {
        Self {
            client,
            peer_source,
            subscribers: Vec::new(),
            class_filter: crate::services::type_index_discovery::uris::AGENT_SKILL.to_string(),
            interval: DEFAULT_CRAWL_INTERVAL,
        }
    }

    /// Override the default crawl interval (primarily for tests).
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Run one crawl pass over all peers, fanning out events per peer.
    ///
    /// Sequential rather than concurrent for now — a real scheduler with
    /// bounded concurrency and per-peer staleness is a C2 follow-up.
    fn do_crawl(&self, ctx: &mut Context<Self>) {
        let peers = self.peer_source.peers();
        if peers.is_empty() {
            debug!("[dojo-discovery] crawl: no peers configured");
            return;
        }
        let client = self.client.clone();
        let subs = self.subscribers.clone();
        let class_filter = self.class_filter.clone();

        debug!("[dojo-discovery] crawl starting: {} peers, class={}", peers.len(), class_filter);

        let fut = async move {
            let mut events = Vec::with_capacity(peers.len());
            for peer in peers {
                let regs = discover_peer_registrations(&client, &peer, &class_filter, None).await;
                events.push(PeerRegistrationsDiscovered {
                    peer_webid: peer,
                    class_filter: class_filter.clone(),
                    registrations: regs,
                });
            }
            events
        };

        ctx.spawn(
            fut.into_actor(self).map(|events, act, _ctx| {
                for ev in events {
                    for sub in &act.subscribers {
                        if sub.try_send(ev.clone()).is_err() {
                            warn!("[dojo-discovery] subscriber dropped; will be pruned next pass");
                        }
                    }
                }
            }),
        );
    }
}

impl Actor for DojoDiscoveryActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            "[dojo-discovery] started (interval={:?}, class={})",
            self.interval, self.class_filter
        );
        // Periodic tick — stub scheduler per ADR-029 read-side. Agent C2
        // swaps this for a proper freshness-aware scheduler.
        ctx.run_interval(self.interval, |act, ctx| {
            act.do_crawl(ctx);
        });
    }
}

impl Handler<SetPeerSource> for DojoDiscoveryActor {
    type Result = ();
    fn handle(&mut self, msg: SetPeerSource, _ctx: &mut Context<Self>) {
        self.peer_source = msg.0;
    }
}

impl Handler<Subscribe> for DojoDiscoveryActor {
    type Result = ();
    fn handle(&mut self, msg: Subscribe, _ctx: &mut Context<Self>) {
        self.subscribers.push(msg.0);
    }
}

impl Handler<CrawlNow> for DojoDiscoveryActor {
    type Result = ();
    fn handle(&mut self, _msg: CrawlNow, ctx: &mut Context<Self>) {
        self.do_crawl(ctx);
    }
}

impl Handler<SetClassFilter> for DojoDiscoveryActor {
    type Result = ();
    fn handle(&mut self, msg: SetClassFilter, _ctx: &mut Context<Self>) {
        self.class_filter = msg.0;
    }
}

impl SupervisedActorTrait for DojoDiscoveryActor {
    fn actor_name() -> &'static str {
        "DojoDiscoveryActor"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_peer_source_returns_configured_set() {
        let mut peers = HashSet::new();
        peers.insert("https://pod.example/alice/profile/card#me".to_string());
        peers.insert("https://pod.example/bob/profile/card#me".to_string());
        let src = StaticPeerSource { peers: peers.clone() };
        assert_eq!(src.peers(), peers);
    }

    #[actix::test]
    async fn actor_starts_and_accepts_empty_peer_source() {
        // No peers → no crawl fan-out, no panics. Validates wiring only;
        // real peer fetches are covered by integration tests that stand up
        // a mock Pod server.
        let client = PodClient::new(reqwest::Client::new(), None);
        let src: Arc<dyn PeerSource> = Arc::new(StaticPeerSource::default());
        let actor = DojoDiscoveryActor::new(client, src)
            .with_interval(Duration::from_secs(3600))
            .start();
        actor.send(CrawlNow).await.unwrap();
    }
}
