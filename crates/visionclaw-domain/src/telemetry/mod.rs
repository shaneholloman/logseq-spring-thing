//! Telemetry domain types — pure data, no I/O
//!
//! Promoted from `visionclaw-server::telemetry::agent_telemetry` per ADR-090 Phase A6 slice 2.
//! Infrastructure (file sink, global logger, emit macros) stays in `visionclaw-server`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::utils::time;

// ── CorrelationId ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_agent_id(agent_id: &str) -> Self {
        Self(format!("agent-{}", agent_id))
    }

    pub fn from_session_uuid(uuid: &str) -> Self {
        Self(format!("session-{}", uuid))
    }

    pub fn from_swarm_id(swarm_id: &str) -> Self {
        Self(format!("swarm-{}", swarm_id))
    }

    pub fn from_client_session(client_session_id: &str) -> Self {
        Self(format!("client-{}", client_session_id))
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── LogLevel ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogLevel {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
}

impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Trace => LogLevel::TRACE,
            log::Level::Debug => LogLevel::DEBUG,
            log::Level::Info => LogLevel::INFO,
            log::Level::Warn => LogLevel::WARN,
            log::Level::Error => LogLevel::ERROR,
        }
    }
}

// ── Position3D ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub magnitude: f32,
}

impl Position3D {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        let magnitude = (x * x + y * y + z * z).sqrt();
        Self { x, y, z, magnitude }
    }

    pub fn origin() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    pub fn is_origin(&self) -> bool {
        self.magnitude < f32::EPSILON
    }
}

// ── TelemetryEvent ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    // When
    pub timestamp: DateTime<Utc>,
    pub timestamp_micros: u128,

    // Trace
    pub correlation_id: CorrelationId,

    // Severity
    pub level: LogLevel,

    // Classification
    pub category: String,
    pub event_type: String,

    // Human-readable
    pub message: String,

    // Arbitrary structured data
    pub metadata: HashMap<String, serde_json::Value>,

    // Agent context
    pub agent_id: Option<String>,
    pub component: String,

    // Performance
    pub duration_ms: Option<f64>,
    pub memory_usage_bytes: Option<u64>,

    // Spatial
    pub position: Option<Position3D>,
    pub position_delta: Option<Position3D>,

    // GPU
    pub gpu_kernel: Option<String>,
    pub gpu_execution_time_ms: Option<f64>,
    pub gpu_memory_mb: Option<f32>,

    // MCP
    pub mcp_message_type: Option<String>,
    pub mcp_direction: Option<String>,
    pub mcp_payload_size: Option<usize>,

    // Session
    pub session_uuid: Option<String>,
    pub swarm_id: Option<String>,
    pub client_session_id: Option<String>,
}

impl TelemetryEvent {
    pub fn new(
        correlation_id: CorrelationId,
        level: LogLevel,
        category: &str,
        event_type: &str,
        message: &str,
        component: &str,
    ) -> Self {
        let now = time::now();
        let timestamp_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();

        Self {
            timestamp: now,
            timestamp_micros,
            correlation_id,
            level,
            category: category.to_string(),
            event_type: event_type.to_string(),
            message: message.to_string(),
            metadata: HashMap::new(),
            agent_id: None,
            component: component.to_string(),
            duration_ms: None,
            memory_usage_bytes: None,
            position: None,
            position_delta: None,
            gpu_kernel: None,
            gpu_execution_time_ms: None,
            gpu_memory_mb: None,
            mcp_message_type: None,
            mcp_direction: None,
            mcp_payload_size: None,
            session_uuid: None,
            swarm_id: None,
            client_session_id: None,
        }
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    pub fn with_session_uuid(mut self, session_uuid: &str) -> Self {
        self.session_uuid = Some(session_uuid.to_string());
        self
    }

    pub fn with_swarm_id(mut self, swarm_id: &str) -> Self {
        self.swarm_id = Some(swarm_id.to_string());
        self
    }

    pub fn with_client_session_id(mut self, client_session_id: &str) -> Self {
        self.client_session_id = Some(client_session_id.to_string());
        self
    }

    pub fn with_agent_id(mut self, agent_id: &str) -> Self {
        self.agent_id = Some(agent_id.to_string());
        self
    }

    pub fn with_position(mut self, position: Position3D) -> Self {
        self.position = Some(position);
        self
    }

    pub fn with_position_delta(mut self, old_pos: Position3D, new_pos: Position3D) -> Self {
        let delta = Position3D::new(
            new_pos.x - old_pos.x,
            new_pos.y - old_pos.y,
            new_pos.z - old_pos.z,
        );
        self.position_delta = Some(delta);
        self.position = Some(new_pos);
        self
    }

    pub fn with_gpu_info(mut self, kernel: &str, execution_time_ms: f64, memory_mb: f32) -> Self {
        self.gpu_kernel = Some(kernel.to_string());
        self.gpu_execution_time_ms = Some(execution_time_ms);
        self.gpu_memory_mb = Some(memory_mb);
        self
    }

    pub fn with_mcp_info(
        mut self,
        message_type: &str,
        direction: &str,
        payload_size: usize,
    ) -> Self {
        self.mcp_message_type = Some(message_type.to_string());
        self.mcp_direction = Some(direction.to_string());
        self.mcp_payload_size = Some(payload_size);
        self
    }

    pub fn with_duration(mut self, duration_ms: f64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_3d() {
        let pos = Position3D::new(3.0, 4.0, 0.0);
        assert_eq!(pos.magnitude, 5.0);

        let origin = Position3D::origin();
        assert!(origin.is_origin());
    }

    #[test]
    fn test_correlation_id() {
        let id = CorrelationId::new();
        assert!(!id.as_str().is_empty());

        let agent_id = CorrelationId::from_agent_id("test_agent");
        assert!(agent_id.as_str().contains("agent-test_agent"));
    }

    #[test]
    fn test_telemetry_event_creation() {
        let correlation_id = CorrelationId::new();
        let event = TelemetryEvent::new(
            correlation_id,
            LogLevel::INFO,
            "test",
            "test_event",
            "Test message",
            "test_component",
        );

        assert_eq!(event.category, "test");
        assert_eq!(event.event_type, "test_event");
        assert_eq!(event.message, "Test message");
        assert_eq!(event.component, "test_component");
    }
}
