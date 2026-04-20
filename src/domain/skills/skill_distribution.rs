//! `SkillDistribution` — scope + WAC references.
//!
//! BC19 invariant 5: distribution widening is a policy-gated audit event.

use serde::{Deserialize, Serialize};

/// Distribution scope. Spec §9 maps these to BC18 `ShareState` as:
/// * `Personal` → `Private`
/// * `Team`, `Company` → `Team` (distinguished by WAC group breadth)
/// * `Public` → `Mesh`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistributionScope {
    Personal,
    Team,
    Company,
    Public,
}

impl DistributionScope {
    /// Returns true if `other` is strictly wider than `self`.
    /// Used to decide whether a distribution change requires policy evaluation.
    pub fn is_widening_to(&self, other: DistributionScope) -> bool {
        fn rank(s: DistributionScope) -> u8 {
            match s {
                DistributionScope::Personal => 0,
                DistributionScope::Team => 1,
                DistributionScope::Company => 2,
                DistributionScope::Public => 3,
            }
        }
        rank(other) > rank(*self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDistribution {
    pub scope: DistributionScope,
    pub allow_list: Vec<String>,
    pub group_ref: Option<String>,
    pub wac_refs: Vec<String>,
}

impl SkillDistribution {
    pub fn personal() -> Self {
        Self {
            scope: DistributionScope::Personal,
            allow_list: Vec::new(),
            group_ref: None,
            wac_refs: Vec::new(),
        }
    }
}
