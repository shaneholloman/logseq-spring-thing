//! BC18 `ContributorProfile` aggregate.
//!
//! Pod-first record of the contributor's role, goals, active projects, and
//! collaborators. Pod is the write-master; the Neo4j projection is a derived
//! read model (DDD §BC18 invariant 4).
//!
//! This aggregate owns its invariants in-memory; the pod write itself is
//! performed by [`super::ContextAssemblyService`]'s `PodContributorPort`
//! adapter at the application layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::events::ContributorProfileUpdatedEvent;
use crate::utils::time;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributorProfile {
    pub webid: String,
    pub display_name: Option<String>,
    pub role: String,
    pub goals: Vec<String>,
    pub active_projects: Vec<String>,
    pub collaborators: Vec<String>,
    pub preferred_partners: Vec<String>,
    pub quiet_mode: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ContributorProfile {
    /// Construct a minimal profile. A new contributor lands here on first
    /// Studio visit; subsequent edits go through [`Self::update`].
    pub fn new(webid: impl Into<String>, role: impl Into<String>) -> Self {
        let now = time::now();
        Self {
            webid: webid.into(),
            display_name: None,
            role: role.into(),
            goals: Vec::new(),
            active_projects: Vec::new(),
            collaborators: Vec::new(),
            preferred_partners: Vec::new(),
            quiet_mode: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Apply a partial update and emit a single
    /// [`ContributorProfileUpdatedEvent`] describing the change.
    pub fn update(&mut self, change: ProfileChange) -> ContributorProfileUpdatedEvent {
        let mut summary_parts: Vec<&str> = Vec::new();
        if let Some(name) = change.display_name {
            self.display_name = Some(name);
            summary_parts.push("display_name");
        }
        if let Some(role) = change.role {
            self.role = role;
            summary_parts.push("role");
        }
        if let Some(goals) = change.goals {
            self.goals = goals;
            summary_parts.push("goals");
        }
        if let Some(projects) = change.active_projects {
            self.active_projects = projects;
            summary_parts.push("active_projects");
        }
        if let Some(collaborators) = change.collaborators {
            self.collaborators = collaborators;
            summary_parts.push("collaborators");
        }
        if let Some(partners) = change.preferred_partners {
            self.preferred_partners = partners;
            summary_parts.push("preferred_partners");
        }
        if let Some(quiet) = change.quiet_mode {
            self.quiet_mode = quiet;
            summary_parts.push("quiet_mode");
        }
        self.updated_at = time::now();
        ContributorProfileUpdatedEvent {
            webid: self.webid.clone(),
            change_summary: summary_parts.join(","),
            timestamp: self.updated_at,
        }
    }
}

/// Partial update payload. All fields `None` leaves the profile untouched.
#[derive(Debug, Default, Clone)]
pub struct ProfileChange {
    pub display_name: Option<String>,
    pub role: Option<String>,
    pub goals: Option<Vec<String>>,
    pub active_projects: Option<Vec<String>>,
    pub collaborators: Option<Vec<String>>,
    pub preferred_partners: Option<Vec<String>>,
    pub quiet_mode: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_update_emits_event() {
        let mut p = ContributorProfile::new("https://alice.example/card#me", "Contributor");
        let evt = p.update(ProfileChange {
            goals: Some(vec!["ship BC18".into()]),
            quiet_mode: Some(true),
            ..Default::default()
        });
        assert_eq!(p.goals.len(), 1);
        assert!(p.quiet_mode);
        assert!(evt.change_summary.contains("goals"));
        assert!(evt.change_summary.contains("quiet_mode"));
    }
}
