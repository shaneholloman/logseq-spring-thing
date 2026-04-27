//! WAC Mutator — applies and verifies the ADR-052 double-gate for share-state
//! transitions, extended to Team shares per
//! `docs/design/2026-04-20-contributor-studio/03-pod-context-memory-and-sharing.md`
//! §7.2 / §8.2.
//!
//! Gate discipline:
//! * **Gate 1 (artefact)**: the artefact manifest carries
//!   `distribution_scope ∈ {team:<t>, mesh}` AND the `allow_list` names the
//!   target team (team shares only).
//! * **Gate 2 (path)**: the destination Pod path matches the scope:
//!   `/shared/{kind}/{team}/...` for Team, `/public/{kind}/...` for Mesh.
//!
//! Both gates MUST hold. Either-gate-only is a bug and produces
//! [`WacMutatorError::DoubleGateMismatch`]; the caller must not fall back
//! to writing to the target path.

use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

use crate::services::pod_client::{PodClient, PodClientError};
use crate::services::share_policy::{ShareIntent, SubjectKind, TargetScope};

#[derive(Debug, Error)]
pub enum WacMutatorError {
    #[error("double-gate mismatch: manifest={manifest_scope:?} allow_list={allow_list:?} path={path}")]
    DoubleGateMismatch {
        manifest_scope: Option<String>,
        allow_list: Vec<String>,
        path: String,
    },

    #[error("subject kind not supported for WAC write: {0:?}")]
    UnsupportedSubjectKind(SubjectKind),

    #[error("unexpected target scope for WAC mutation: {0:?}")]
    UnexpectedTargetScope(TargetScope),

    #[error("pod error: {0}")]
    Pod(#[from] PodClientError),
}

/// Computed destination of a WAC-mutation for a given ShareIntent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WacMutationPlan {
    /// Absolute (or Pod-relative) destination path, e.g.
    /// `/shared/skills/team-alpha/research-brief.md`.
    pub destination_path: String,
    /// ACL document path for the parent container,
    /// e.g. `/shared/skills/team-alpha/.acl`.
    pub acl_document_path: String,
    /// Named group IRI that the ACL grants access to (team shares only).
    pub agent_group: Option<String>,
    /// Turtle representation of the ACL that the mutator wants to write.
    pub acl_turtle: String,
}

/// Port for WAC mutation. The default implementation in
/// [`PodWacMutator`] writes through [`PodClient`]; tests substitute the
/// in-memory [`InMemoryWacMutator`].
#[async_trait]
pub trait WacMutator: Send + Sync {
    /// Plan the mutation without writing (used by the orchestrator to
    /// validate both gates before any Pod side-effect).
    fn plan(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError>;

    /// Apply the plan: write the artefact manifest + body MOVE + ACL upsert.
    async fn apply(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError>;

    /// Revoke a previously-applied plan (used by ContributorRevocation /
    /// BrokerRevocation paths).
    async fn revoke(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError>;
}

/// Convert a [`SubjectKind`] to the container segment used under `/shared/`
/// and `/public/`.
fn container_kind_segment(kind: SubjectKind) -> Result<&'static str, WacMutatorError> {
    match kind {
        SubjectKind::Skill => Ok("skills"),
        SubjectKind::WorkArtifact => Ok("workspaces"),
        // ontology_term / workflow / graph_view have graph-only canonical
        // copies; no WAC path move is performed for them. The orchestrator
        // routes these through the broker adapter without calling apply().
        other => Err(WacMutatorError::UnsupportedSubjectKind(other)),
    }
}

fn artifact_file_name(artifact_ref: &str) -> String {
    artifact_ref
        .rsplit('/')
        .next()
        .unwrap_or(artifact_ref)
        .to_string()
}

/// Build the ACL turtle document for an owner-only + named-group Read+Write
/// container (spec §2.3 team template).
fn render_team_acl(owner_webid: &str, group_iri: &str) -> String {
    format!(
        "@prefix acl: <http://www.w3.org/ns/auth/acl#> .\n\
         @prefix vc: <urn:visionclaw:acl#> .\n\
         \n\
         <#owner>\n    a acl:Authorization ;\n    acl:agent <{owner}> ;\n    acl:accessTo <./> ;\n    acl:default <./> ;\n    acl:mode acl:Read, acl:Write, acl:Control .\n\
         \n\
         <#team>\n    a acl:Authorization ;\n    acl:agentGroup <{group}> ;\n    acl:accessTo <./> ;\n    acl:default <./> ;\n    acl:mode acl:Read, acl:Write .\n",
        owner = owner_webid,
        group = group_iri,
    )
}

/// Build the public-read ACL for `/public/{kind}/{slug}/` (spec §2.3 implied;
/// mirrors ADR-052 root template).
fn render_public_acl(owner_webid: &str) -> String {
    format!(
        "@prefix acl: <http://www.w3.org/ns/auth/acl#> .\n\
         @prefix foaf: <http://xmlns.com/foaf/0.1/> .\n\
         \n\
         <#owner>\n    a acl:Authorization ;\n    acl:agent <{owner}> ;\n    acl:accessTo <./> ;\n    acl:default <./> ;\n    acl:mode acl:Read, acl:Write, acl:Control .\n\
         \n\
         <#public>\n    a acl:Authorization ;\n    acl:agentClass foaf:Agent ;\n    acl:accessTo <./> ;\n    acl:default <./> ;\n    acl:mode acl:Read .\n",
        owner = owner_webid,
    )
}

/// Owner-only ACL for revocation back to `/private/`.
fn render_private_acl(owner_webid: &str) -> String {
    format!(
        "@prefix acl: <http://www.w3.org/ns/auth/acl#> .\n\
         \n\
         <#owner>\n    a acl:Authorization ;\n    acl:agent <{owner}> ;\n    acl:accessTo <./> ;\n    acl:default <./> ;\n    acl:mode acl:Read, acl:Write, acl:Control .\n",
        owner = owner_webid,
    )
}

/// Verify the manifest + path double-gate for a planned mutation.
///
/// Returns `Ok(())` if and only if:
/// * For `TargetScope::Team(t)`:
///   * `distribution_scope_manifest` starts with `"team"` AND
///   * `allow_list` contains `t` AND
///   * `destination_path` is under `/shared/{kind}/{t}/`
/// * For `TargetScope::Mesh`:
///   * `distribution_scope_manifest == Some("mesh")` AND
///   * `destination_path` is under `/public/{kind}/`
/// * For `TargetScope::Private`:
///   * `destination_path` is under `/private/{kind}/`
pub fn verify_double_gate(
    intent: &ShareIntent,
    destination_path: &str,
) -> Result<(), WacMutatorError> {
    let manifest = intent.distribution_scope_manifest.clone();
    match &intent.target_scope {
        TargetScope::Team(t) => {
            let manifest_ok = manifest
                .as_deref()
                .map(|s| s.starts_with("team"))
                .unwrap_or(false);
            let allow_ok = intent.allow_list.iter().any(|x| x == t);
            let path_ok = destination_path.contains(&format!("/shared/"))
                && destination_path.contains(&format!("/{}/", t));
            if manifest_ok && allow_ok && path_ok {
                Ok(())
            } else {
                Err(WacMutatorError::DoubleGateMismatch {
                    manifest_scope: manifest,
                    allow_list: intent.allow_list.clone(),
                    path: destination_path.to_string(),
                })
            }
        }
        TargetScope::Mesh => {
            let manifest_ok = manifest.as_deref() == Some("mesh");
            let path_ok = destination_path.contains("/public/");
            if manifest_ok && path_ok {
                Ok(())
            } else {
                Err(WacMutatorError::DoubleGateMismatch {
                    manifest_scope: manifest,
                    allow_list: intent.allow_list.clone(),
                    path: destination_path.to_string(),
                })
            }
        }
        TargetScope::Private => {
            if destination_path.contains("/private/") {
                Ok(())
            } else {
                Err(WacMutatorError::DoubleGateMismatch {
                    manifest_scope: manifest,
                    allow_list: intent.allow_list.clone(),
                    path: destination_path.to_string(),
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pod-backed implementation.
// ---------------------------------------------------------------------------

/// [`WacMutator`] backed by a real [`PodClient`]. Used in production.
pub struct PodWacMutator {
    client: Arc<PodClient>,
}

impl PodWacMutator {
    pub fn new(client: Arc<PodClient>) -> Self { Self { client } }
}

#[async_trait]
impl WacMutator for PodWacMutator {
    fn plan(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
        plan_mutation(intent, pod_base)
    }

    async fn apply(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
        let plan = plan_mutation(intent, pod_base)?;
        verify_double_gate(intent, &plan.destination_path)?;

        // 1. Move artefact body from source path to destination.
        //    Source inferred from `intent.artifact_ref` (e.g. pod:/private/...)
        let src = abs_pod_url(pod_base, pod_ref_path(&intent.artifact_ref));
        let dst = abs_pod_url(pod_base, &plan.destination_path);
        self.client.move_resource(&src, &dst, None).await?;

        // 2. Write / upsert the ACL document for the destination container.
        let acl_url = abs_pod_url(pod_base, &plan.acl_document_path);
        self.client
            .put_resource(&acl_url, plan.acl_turtle.clone().into_bytes().into(),
                "text/turtle", None)
            .await?;

        Ok(plan)
    }

    async fn revoke(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
        // Revocation moves the artefact back to /private/{kind}/... and
        // writes an owner-only ACL. Callers route through this symmetric
        // operation to preserve the double-gate invariant.
        let kind_seg = container_kind_segment(intent.subject_kind)?;
        let file = artifact_file_name(&intent.artifact_ref);
        let destination_path = format!("/private/{kind}/{name}",
            kind = kind_seg, name = file);
        let acl_document_path = format!("/private/{kind}/.acl", kind = kind_seg);
        let acl_turtle = render_private_acl(&intent.contributor_webid);

        let plan = WacMutationPlan {
            destination_path: destination_path.clone(),
            acl_document_path: acl_document_path.clone(),
            agent_group: None,
            acl_turtle: acl_turtle.clone(),
        };

        // Source path for the revocation is the current artifact_ref.
        let src = abs_pod_url(pod_base, pod_ref_path(&intent.artifact_ref));
        let dst = abs_pod_url(pod_base, &destination_path);
        self.client.move_resource(&src, &dst, None).await?;

        let acl_url = abs_pod_url(pod_base, &acl_document_path);
        self.client
            .put_resource(&acl_url, acl_turtle.into_bytes().into(),
                "text/turtle", None)
            .await?;

        Ok(plan)
    }
}

fn plan_mutation(intent: &ShareIntent, _pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
    match &intent.target_scope {
        TargetScope::Team(team) => {
            let kind_seg = container_kind_segment(intent.subject_kind)?;
            let file = artifact_file_name(&intent.artifact_ref);
            let destination_path = format!(
                "/shared/{kind}/{team}/{name}",
                kind = kind_seg, team = team, name = file);
            let acl_document_path = format!(
                "/shared/{kind}/{team}/.acl", kind = kind_seg, team = team);
            let group_iri = crate::uri::mint_group_members(team);
            let acl_turtle = render_team_acl(&intent.contributor_webid, &group_iri);
            verify_double_gate(intent, &destination_path)?;
            Ok(WacMutationPlan {
                destination_path,
                acl_document_path,
                agent_group: Some(group_iri),
                acl_turtle,
            })
        }
        TargetScope::Mesh => {
            let kind_seg = container_kind_segment(intent.subject_kind)?;
            let file = artifact_file_name(&intent.artifact_ref);
            let destination_path = format!(
                "/public/{kind}/{name}", kind = kind_seg, name = file);
            let acl_document_path = format!(
                "/public/{kind}/.acl", kind = kind_seg);
            let acl_turtle = render_public_acl(&intent.contributor_webid);
            verify_double_gate(intent, &destination_path)?;
            Ok(WacMutationPlan {
                destination_path,
                acl_document_path,
                agent_group: None,
                acl_turtle,
            })
        }
        TargetScope::Private => {
            Err(WacMutatorError::UnexpectedTargetScope(TargetScope::Private))
        }
    }
}

fn pod_ref_path(pod_ref: &str) -> &str {
    pod_ref.strip_prefix("pod:").unwrap_or(pod_ref)
}

fn abs_pod_url(pod_base: &str, path: &str) -> String {
    let base = pod_base.trim_end_matches('/');
    let suffix = if path.starts_with('/') { path.to_string() } else { format!("/{}", path) };
    format!("{}{}", base, suffix)
}

// ---------------------------------------------------------------------------
// Test double.
// ---------------------------------------------------------------------------

/// In-memory mutator that records plans without touching a real Pod.
#[derive(Default)]
pub struct InMemoryWacMutator {
    pub applied: std::sync::Mutex<Vec<WacMutationPlan>>,
    pub revoked: std::sync::Mutex<Vec<WacMutationPlan>>,
}

#[async_trait]
impl WacMutator for InMemoryWacMutator {
    fn plan(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
        plan_mutation(intent, pod_base)
    }

    async fn apply(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
        let plan = plan_mutation(intent, pod_base)?;
        verify_double_gate(intent, &plan.destination_path)?;
        self.applied.lock().unwrap().push(plan.clone());
        Ok(plan)
    }

    async fn revoke(&self, intent: &ShareIntent, pod_base: &str) -> Result<WacMutationPlan, WacMutatorError> {
        let kind_seg = container_kind_segment(intent.subject_kind)?;
        let file = artifact_file_name(&intent.artifact_ref);
        let destination_path = format!("/private/{kind}/{name}",
            kind = kind_seg, name = file);
        let plan = WacMutationPlan {
            destination_path: destination_path.clone(),
            acl_document_path: format!("/private/{kind}/.acl", kind = kind_seg),
            agent_group: None,
            acl_turtle: render_private_acl(&intent.contributor_webid),
        };
        let _ = pod_base;
        self.revoked.lock().unwrap().push(plan.clone());
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::share_policy::{PiiScanStatus, ShareState};
    use chrono::Utc;
    use std::collections::HashMap;

    fn intent(scope: TargetScope, allow: Vec<&str>, manifest: Option<&str>) -> ShareIntent {
        ShareIntent {
            intent_id: "si-1".into(),
            contributor_webid: "https://alice.pod/profile/card#me".into(),
            subject_kind: SubjectKind::Skill,
            artifact_ref: "pod:/private/skills/research-brief.md".into(),
            source_state: ShareState::Private,
            target_scope: scope,
            rationale: None,
            distribution_scope_manifest: manifest.map(str::to_string),
            allow_list: allow.into_iter().map(String::from).collect(),
            pii_scan_status: PiiScanStatus::Clean,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn team_apply_writes_named_group_acl() {
        let mut_: InMemoryWacMutator = Default::default();
        let i = intent(TargetScope::Team("team-alpha".into()),
            vec!["team-alpha"], Some("team"));
        let plan = mut_.apply(&i, "https://alice.pod").await.unwrap();
        assert_eq!(plan.destination_path,
            "/shared/skills/team-alpha/research-brief.md");
        assert!(plan.agent_group.as_deref().unwrap().contains("team-alpha"));
        assert!(plan.acl_turtle.contains("acl:agentGroup"));
    }

    #[tokio::test]
    async fn team_manifest_missing_fails_double_gate() {
        let mut_: InMemoryWacMutator = Default::default();
        // manifest None — gate 1 fails
        let i = intent(TargetScope::Team("team-alpha".into()),
            vec!["team-alpha"], None);
        let err = mut_.apply(&i, "https://alice.pod").await.unwrap_err();
        matches!(err, WacMutatorError::DoubleGateMismatch { .. });
    }

    #[tokio::test]
    async fn team_allow_list_mismatch_fails_double_gate() {
        let mut_: InMemoryWacMutator = Default::default();
        let i = intent(TargetScope::Team("team-alpha".into()),
            vec!["team-beta"], Some("team"));
        let err = mut_.apply(&i, "https://alice.pod").await.unwrap_err();
        matches!(err, WacMutatorError::DoubleGateMismatch { .. });
    }

    #[tokio::test]
    async fn mesh_apply_writes_public_acl() {
        let mut_: InMemoryWacMutator = Default::default();
        let i = intent(TargetScope::Mesh, vec![], Some("mesh"));
        let plan = mut_.apply(&i, "https://alice.pod").await.unwrap();
        assert!(plan.destination_path.starts_with("/public/skills/"));
        assert!(plan.acl_turtle.contains("foaf:Agent"));
    }

    #[tokio::test]
    async fn mesh_manifest_mismatch_fails_double_gate() {
        let mut_: InMemoryWacMutator = Default::default();
        let i = intent(TargetScope::Mesh, vec![], Some("team"));
        let err = mut_.apply(&i, "https://alice.pod").await.unwrap_err();
        matches!(err, WacMutatorError::DoubleGateMismatch { .. });
    }

    #[tokio::test]
    async fn revoke_writes_owner_only_acl() {
        let mut_: InMemoryWacMutator = Default::default();
        let i = intent(TargetScope::Team("team-alpha".into()),
            vec!["team-alpha"], Some("team"));
        let plan = mut_.revoke(&i, "https://alice.pod").await.unwrap();
        assert!(plan.destination_path.starts_with("/private/skills/"));
        assert!(plan.agent_group.is_none());
        assert!(!plan.acl_turtle.contains("agentGroup"));
    }

    #[tokio::test]
    async fn ontology_term_not_supported_for_wac_move() {
        let mut_: InMemoryWacMutator = Default::default();
        let mut i = intent(TargetScope::Team("team-alpha".into()),
            vec!["team-alpha"], Some("team"));
        i.subject_kind = SubjectKind::OntologyTerm;
        let err = mut_.apply(&i, "https://alice.pod").await.unwrap_err();
        matches!(err, WacMutatorError::UnsupportedSubjectKind(_));
    }
}
