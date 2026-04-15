//! In-memory policy evaluation service.
//!
//! Implements the [`PolicyEngine`] port with a set of hard-coded default rules.
//! This is a P4 stepping-stone — production deployments will swap in a
//! database-backed or OPA-backed adapter behind the same trait.

use async_trait::async_trait;

use crate::models::enterprise::{PolicyAction, PolicyContext, PolicyEvaluation, PolicyOutcome};
use crate::ports::policy_engine::{PolicyEngine, PolicyError};

/// A single policy rule configuration.
struct PolicyRuleConfig {
    id: String,
    name: String,
    /// Serialised [`PolicyAction`] variant name, or `"*"` to match all actions.
    action_pattern: String,
    outcome: PolicyOutcome,
    /// Optional confidence threshold — when present and the context confidence
    /// meets or exceeds it, the outcome is promoted to [`PolicyOutcome::Allow`].
    threshold: Option<f64>,
    enabled: bool,
}

/// In-memory policy engine with a fixed rule-set.
pub struct InMemoryPolicyEngine {
    rules: Vec<PolicyRuleConfig>,
}

impl InMemoryPolicyEngine {
    pub fn new() -> Self {
        Self {
            rules: vec![
                PolicyRuleConfig {
                    id: "confidence_threshold".into(),
                    name: "Confidence Threshold".into(),
                    action_pattern: "deploy_workflow".into(),
                    outcome: PolicyOutcome::Escalate,
                    threshold: Some(0.7),
                    enabled: true,
                },
                PolicyRuleConfig {
                    id: "separation_of_duty".into(),
                    name: "Separation of Duty".into(),
                    action_pattern: "approve_workflow".into(),
                    outcome: PolicyOutcome::Deny,
                    threshold: None,
                    enabled: true,
                },
                PolicyRuleConfig {
                    id: "escalation_review".into(),
                    name: "Escalation Review".into(),
                    action_pattern: "escalate_case".into(),
                    outcome: PolicyOutcome::Escalate,
                    threshold: None,
                    enabled: true,
                },
                PolicyRuleConfig {
                    id: "override_audit".into(),
                    name: "Override Audit".into(),
                    action_pattern: "override_decision".into(),
                    outcome: PolicyOutcome::Warn,
                    threshold: None,
                    enabled: true,
                },
                PolicyRuleConfig {
                    id: "connector_access".into(),
                    name: "Connector Access".into(),
                    action_pattern: "access_connector".into(),
                    outcome: PolicyOutcome::Allow,
                    threshold: None,
                    enabled: true,
                },
                PolicyRuleConfig {
                    id: "policy_modification".into(),
                    name: "Policy Modification Guard".into(),
                    action_pattern: "modify_policy".into(),
                    outcome: PolicyOutcome::Deny,
                    threshold: None,
                    enabled: true,
                },
            ],
        }
    }

    /// Convert a [`PolicyAction`] to its snake_case string form.
    fn action_to_string(action: &PolicyAction) -> String {
        match action {
            PolicyAction::ApproveWorkflow => "approve_workflow".into(),
            PolicyAction::DeployWorkflow => "deploy_workflow".into(),
            PolicyAction::EscalateCase => "escalate_case".into(),
            PolicyAction::OverrideDecision => "override_decision".into(),
            PolicyAction::AccessConnector => "access_connector".into(),
            PolicyAction::ModifyPolicy => "modify_policy".into(),
        }
    }
}

impl Default for InMemoryPolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PolicyEngine for InMemoryPolicyEngine {
    async fn evaluate(
        &self,
        context: &PolicyContext,
    ) -> Result<Vec<PolicyEvaluation>, PolicyError> {
        let action_str = Self::action_to_string(&context.action);

        let mut evaluations = Vec::new();
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            if rule.action_pattern != "*" && rule.action_pattern != action_str {
                continue;
            }

            let outcome = match (&rule.outcome, rule.threshold, context.confidence) {
                (PolicyOutcome::Escalate, Some(threshold), Some(confidence))
                    if confidence >= threshold =>
                {
                    PolicyOutcome::Allow
                }
                (outcome, _, _) => outcome.clone(),
            };

            let reasoning = match &outcome {
                PolicyOutcome::Allow => format!("{}: passed", rule.name),
                PolicyOutcome::Deny => format!("{}: blocked", rule.name),
                PolicyOutcome::Escalate => format!("{}: requires broker review", rule.name),
                PolicyOutcome::Warn => format!("{}: warning", rule.name),
            };

            evaluations.push(PolicyEvaluation {
                rule_id: rule.id.clone(),
                outcome,
                reasoning,
                confidence: context.confidence.unwrap_or(0.0),
                evaluated_at: chrono::Utc::now().to_rfc3339(),
            });
        }

        Ok(evaluations)
    }

    async fn decide(&self, context: &PolicyContext) -> Result<PolicyOutcome, PolicyError> {
        let evaluations = self.evaluate(context).await?;

        // First Deny wins, then Escalate, else Allow
        if evaluations
            .iter()
            .any(|e| e.outcome == PolicyOutcome::Deny)
        {
            return Ok(PolicyOutcome::Deny);
        }
        if evaluations
            .iter()
            .any(|e| e.outcome == PolicyOutcome::Escalate)
        {
            return Ok(PolicyOutcome::Escalate);
        }
        Ok(PolicyOutcome::Allow)
    }
}
