use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single Logseq block parsed to full fidelity per ADR-068.
///
/// Matches matryca's data model: `:block/uuid`, `:block/parent`, `:block/left`,
/// `:block/refs`, `:block/path-refs`, `:block/journal-day`, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockNode {
    pub uuid: String,
    pub content: String,
    pub clean_text: String,
    pub parent_id: Option<String>,
    pub left_id: Option<String>,
    pub indent_level: u32,
    #[serde(default)]
    pub properties: HashMap<String, String>,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub path_refs: Vec<String>,
    pub task_status: Option<TaskStatus>,
    pub scheduled: Option<String>,
    pub deadline: Option<String>,
    pub repeater: Option<String>,
    pub created_at: Option<String>,
    pub journal_day: Option<u32>,
    pub block_index: u32,
    #[serde(default)]
    pub clock_entries: Vec<ClockEntry>,
    pub truncated: bool,
    pub cyclic_embed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TaskStatus {
    Todo,
    Doing,
    Done,
    Now,
    Later,
    Wait,
    Cancelled,
}

impl TaskStatus {
    pub fn from_marker(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "TODO" => Some(Self::Todo),
            "DOING" => Some(Self::Doing),
            "DONE" => Some(Self::Done),
            "NOW" => Some(Self::Now),
            "LATER" => Some(Self::Later),
            "WAIT" | "WAITING" => Some(Self::Wait),
            "CANCELLED" | "CANCELED" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockEntry {
    pub start: String,
    pub end: Option<String>,
    pub duration_min: Option<u32>,
}
