use actix::prelude::*;
use actix::ResponseActFuture;
use actix_web::web;
use actix_web_actors::ws::WebsocketContext;
use bytestring::ByteString;
use bytemuck;
use futures::StreamExt;
use log::{debug, error, info};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;
use actix_web_actors::ws;  // Add ws import
use actix::StreamHandler;  // Add StreamHandler import


use crate::AppState;
use crate::models::node::GPUNode;
use crate::models::simulation_params::{SimulationMode, SimulationParams};
use crate::models::position_update::NodePositionVelocity;
use crate::utils::websocket_messages::{
    MessageHandler, OpenAIConnected, OpenAIConnectionFailed, OpenAIMessage, SendBinary, SendText,
    ServerMessage,
};
use crate::utils::websocket_openai::OpenAIWebSocket;

// Constants for timing and performance
pub const OPENAI_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const GPU_UPDATE_INTERVAL: Duration = Duration::from_millis(16); // ~60fps for smooth updates

// Message type for GPU position updates
#[derive(Message)]
#[rtype(result = "()")]
pub struct GpuUpdate;

/// WebSocket session actor handling client communication
pub struct WebSocketSession {
    pub state: web::Data<AppState>,
    pub tts_method: String,
    pub openai_ws: Option<Addr<OpenAIWebSocket>>,
    pub simulation_mode: SimulationMode,
    pub conversation_id: Option<Arc<Mutex<Option<String>>>>,
}

impl WebSocketSession {
    pub fn new(state: web::Data<AppState>) -> Self {
        Self {
            state,
            tts_method: String::from("local"),
            openai_ws: None,
            simulation_mode: SimulationMode::Remote,
            conversation_id: Some(Arc::new(Mutex::new(None))),
        }
    }
}

impl Actor for WebSocketSession {
    type Context = WebsocketContext<Self>;
}

// Add StreamHandler implementation for WebSocket messages
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                debug!("Ping received");
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                debug!("Pong received");
            }
            Ok(ws::Message::Text(text)) => {
                debug!("Text message received: {}", text);
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    match value.get("type").and_then(|t| t.as_str()) {
                        Some("chat") => {
                            if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
                                let use_openai = value.get("useOpenAI")
                                    .and_then(|o| o.as_bool())
                                    .unwrap_or(false);
                                self.handle_chat_message(ctx, message.to_string(), use_openai);
                            }
                        }
                        Some("simulation_mode") => {
                            if let Some(mode) = value.get("mode").and_then(|m| m.as_str()) {
                                self.handle_simulation_mode(ctx, mode);
                            }
                        }
                        Some("layout") => {
                            if let Ok(params) = serde_json::from_value::<SimulationParams>(value["params"].clone()) {
                                self.handle_layout(ctx, params);
                            }
                        }
                        Some("fisheye") => {
                            let enabled = value.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false);
                            let strength = value.get("strength").and_then(|s| s.as_f64()).unwrap_or(1.0) as f32;
                            let focus_point = value.get("focusPoint")
                                .and_then(|f| f.as_array())
                                .and_then(|arr| {
                                    if arr.len() == 3 {
                                        Some([
                                            arr[0].as_f64().unwrap_or(0.0) as f32,
                                            arr[1].as_f64().unwrap_or(0.0) as f32,
                                            arr[2].as_f64().unwrap_or(0.0) as f32,
                                        ])
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or([0.0, 0.0, 0.0]);
                            let radius = value.get("radius").and_then(|r| r.as_f64()).unwrap_or(1.0) as f32;
                            self.handle_fisheye_settings(ctx, enabled, strength, focus_point, radius);
                        }
                        Some("initial_data") => {
                            self.handle_initial_data(ctx);
                        }
                        _ => {
                            error!("Unknown message type received");
                            let error_message = ServerMessage::Error {
                                message: "Unknown message type".to_string(),
                                code: Some("UNKNOWN_MESSAGE_TYPE".to_string())
                            };
                            if let Ok(error_str) = serde_json::to_string(&error_message) {
                                ctx.text(ByteString::from(error_str));
                            }
                        }
                    }
                }
            }
            Ok(ws::Message::Binary(bin)) => {
                debug!("Binary message received: {} bytes", bin.len());
                // Handle binary messages if needed
            }
            Ok(ws::Message::Close(reason)) => {
                debug!("Client disconnected: {:?}", reason);
                ctx.close(reason);
                ctx.stop();
            }
            Ok(ws::Message::Continuation(_)) => {
                debug!("Continuation frame received");
                // Handle continuation frames if needed
            }
            Ok(ws::Message::Nop) => {
                debug!("Nop frame received");
            }
            Err(e) => {
                error!("Error in WebSocket message handling: {}", e);
                ctx.stop();
            }
        }
    }
}

impl MessageHandler for WebSocketSession {}

/// Helper function to convert hex color to proper format
/// Handles various input formats (0x, #, or raw hex) and normalizes to #RRGGBB
pub fn format_color(color: &str) -> String {
    let color = color.trim_matches('"')
        .trim_start_matches("0x")
        .trim_start_matches('#');
    
    // Handle rgba format
    if color.starts_with("rgba(") {
        return color.to_string();
    }
    
    // Handle regular hex colors
    format!("#{}", color)
}

/// Helper function to convert GPU nodes to binary position updates
/// Creates efficient binary format for network transfer (24 bytes per node)
pub fn positions_to_binary(nodes: &[GPUNode]) -> Vec<u8> {
    let mut binary_data = Vec::with_capacity(nodes.len() * std::mem::size_of::<NodePositionVelocity>());
    for node in nodes {
        // Convert to position update format (24 bytes)
        let update = NodePositionVelocity {
            x: node.x,
            y: node.y,
            z: node.z,
            vx: node.vx,
            vy: node.vy,
            vz: node.vz,
        };
        // Use as_bytes() since NodePositionVelocity is Pod
        binary_data.extend_from_slice(bytemuck::bytes_of(&update));
    }
    binary_data
}

// WebSocket session handler trait defining main message handlers
pub trait WebSocketSessionHandler {
    fn start_gpu_updates(&self, ctx: &mut WebsocketContext<WebSocketSession>);
    fn handle_chat_message(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, message: String, use_openai: bool);
    fn handle_simulation_mode(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, mode: &str);
    fn handle_layout(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, params: SimulationParams);
    fn handle_initial_data(&mut self, ctx: &mut WebsocketContext<WebSocketSession>);
    fn handle_fisheye_settings(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, enabled: bool, strength: f32, focus_point: [f32; 3], radius: f32);
}

// Main WebSocket session handler implementation
impl WebSocketSessionHandler for WebSocketSession {
    fn handle_initial_data(&mut self, ctx: &mut WebsocketContext<WebSocketSession>) {
        let state = self.state.clone();
        let ctx_addr = ctx.address();

        let fut = async move {
            let graph_data = state.graph_data.read().await;
            let settings = state.settings.read().await;
            
            // Send graph data using ServerMessage enum, ensuring metadata is included
            let graph_update = ServerMessage::GraphUpdate {
                graph_data: json!({
                    "nodes": graph_data.nodes.iter().map(|node| {
                        json!({
                            "id": node.id,
                            "label": node.label,
                            "position": [node.x, node.y, node.z],
                            "velocity": [node.vx, node.vy, node.vz],
                            "size": node.size,
                            "color": node.color,
                            "type": node.node_type,
                            "metadata": node.metadata,
                            "userData": node.user_data,
                            "weight": node.weight,
                            "group": node.group
                        })
                    }).collect::<Vec<_>>(),
                    "edges": graph_data.edges.iter().map(|edge| {
                        json!({
                            "source": edge.source,
                            "target": edge.target,
                            "weight": edge.weight,
                            "width": edge.width,
                            "color": edge.color,
                            "type": edge.edge_type,
                            "metadata": edge.metadata,
                            "userData": edge.user_data,
                            "directed": edge.directed.unwrap_or(false)
                        })
                    }).collect::<Vec<_>>(),
                    "metadata": &graph_data.metadata
                })
            };
            if let Ok(graph_str) = serde_json::to_string(&graph_update) {
                ctx_addr.do_send(SendText(graph_str));
            }

            // Rest of the function remains the same...
            // Send settings using proper ServerMessage format
            let settings_update = ServerMessage::SettingsUpdated {
                settings: json!({
                    "visualization": {
                        "nodeColor": format_color(&settings.visualization.node_color),
                        "edgeColor": format_color(&settings.visualization.edge_color),
                        "hologramColor": format_color(&settings.visualization.hologram_color),
                        "minNodeSize": settings.visualization.min_node_size,
                        "maxNodeSize": settings.visualization.max_node_size,
                        "hologramScale": settings.visualization.hologram_scale,
                        "hologramOpacity": settings.visualization.hologram_opacity,
                        "edgeOpacity": settings.visualization.edge_opacity,
                        "fogDensity": settings.visualization.fog_density,
                        "nodeMaterial": {
                            "metalness": settings.visualization.node_material_metalness,
                            "roughness": settings.visualization.node_material_roughness,
                            "clearcoat": settings.visualization.node_material_clearcoat,
                            "clearcoatRoughness": settings.visualization.node_material_clearcoat_roughness,
                            "opacity": settings.visualization.node_material_opacity,
                            "emissiveMin": settings.visualization.node_emissive_min_intensity,
                            "emissiveMax": settings.visualization.node_emissive_max_intensity
                        },
                        "physics": {
                            "iterations": settings.visualization.force_directed_iterations,
                            "spring": settings.visualization.force_directed_spring,
                            "repulsion": settings.visualization.force_directed_repulsion,
                            "attraction": settings.visualization.force_directed_attraction,
                            "damping": settings.visualization.force_directed_damping
                        },
                        "bloom": {
                            "nodeStrength": settings.bloom.node_bloom_strength,
                            "nodeRadius": settings.bloom.node_bloom_radius,
                            "nodeThreshold": settings.bloom.node_bloom_threshold,
                            "edgeStrength": settings.bloom.edge_bloom_strength,
                            "edgeRadius": settings.bloom.edge_bloom_radius,
                            "edgeThreshold": settings.bloom.edge_bloom_threshold,
                            "envStrength": settings.bloom.environment_bloom_strength,
                            "envRadius": settings.bloom.environment_bloom_radius,
                            "envThreshold": settings.bloom.environment_bloom_threshold
                        }
                    },
                    "fisheye": {
                        "enabled": settings.fisheye.enabled,
                        "strength": settings.fisheye.strength,
                        "radius": settings.fisheye.radius,
                        "focusPoint": [
                            settings.fisheye.focus_x,
                            settings.fisheye.focus_y,
                            settings.fisheye.focus_z
                        ]
                    }
                })
            };
            if let Ok(settings_str) = serde_json::to_string(&settings_update) {
                ctx_addr.do_send(SendText(settings_str));
            }

            // Send completion
            let completion = json!({
                "type": "completion",
                "message": "Initial data sent"
            });
            if let Ok(completion_str) = serde_json::to_string(&completion) {
                ctx_addr.do_send(SendText(completion_str));
            }
        };

        ctx.spawn(fut.into_actor(self));
        
        // Set simulation mode to remote and start GPU updates
        self.simulation_mode = SimulationMode::Remote;
        if self.state.gpu_compute.is_some() {
            self.start_gpu_updates(ctx);
        }
    }

    // Start periodic GPU updates at 60fps
    fn start_gpu_updates(&self, ctx: &mut WebsocketContext<WebSocketSession>) {
        let addr = ctx.address();
        ctx.run_interval(GPU_UPDATE_INTERVAL, move |_, _| {
            addr.do_send(GpuUpdate);
        });
    }

    // Handle chat messages and TTS responses
    fn handle_chat_message(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, message: String, use_openai: bool) {
        let state = self.state.clone();
        let conversation_id = self.conversation_id.clone();
        let ctx_addr = ctx.address();
        let settings = self.state.settings.clone();
        let weak_addr = ctx.address().downgrade();

        let fut = async move {
            let conv_id = if let Some(conv_arc) = conversation_id {
                if let Some(id) = conv_arc.lock().unwrap().clone() {
                    id
                } else {
                    match state.ragflow_service.create_conversation("default_user".to_string()).await {
                        Ok(new_id) => new_id,
                        Err(e) => {
                            error!("Failed to create conversation: {}", e);
                            return;
                        }
                    }
                }
            } else {
                error!("No conversation ID available");
                return;
            };

            match state.ragflow_service.send_message(
                conv_id.clone(),
                message.clone(),
                false,
                None,
                false,
            ).await {
                Ok(mut stream) => {
                    debug!("RAGFlow service initialized for conversation {}", conv_id);
                    
                    if let Some(result) = stream.next().await {
                        match result {
                            Ok(text) => {
                                debug!("Received text response from RAGFlow: {}", text);
                                
                                if use_openai {
                                    debug!("Creating OpenAI WebSocket for TTS");
                                    let openai_ws = OpenAIWebSocket::new(ctx_addr.clone(), settings);
                                    let addr = openai_ws.start();
                                    
                                    debug!("Waiting for OpenAI WebSocket to be ready");
                                    tokio::time::sleep(OPENAI_CONNECT_TIMEOUT).await;
                                    
                                    debug!("Sending text to OpenAI TTS: {}", text);
                                    addr.do_send(OpenAIMessage(text));
                                } else {
                                    debug!("Using local TTS service");
                                    if let Err(e) = state.speech_service.send_message(text).await {
                                        error!("Failed to generate speech: {}", e);
                                        let error_message = ServerMessage::Error {
                                            message: format!("Failed to generate speech: {}", e),
                                            code: Some("SPEECH_GENERATION_ERROR".to_string())
                                        };
                                        if let Ok(error_str) = serde_json::to_string(&error_message) {
                                            ctx_addr.do_send(SendText(error_str));
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                error!("Error processing RAGFlow response: {}", e);
                                let error_message = ServerMessage::Error {
                                    message: format!("Error processing RAGFlow response: {}", e),
                                    code: Some("RAGFLOW_PROCESSING_ERROR".to_string())
                                };
                                if let Ok(error_str) = serde_json::to_string(&error_message) {
                                    ctx_addr.do_send(SendText(error_str));
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to send message to RAGFlow: {}", e);
                    let error_message = ServerMessage::Error {
                        message: format!("Failed to send message to RAGFlow: {}", e),
                        code: Some("RAGFLOW_SEND_ERROR".to_string())
                    };
                    if let Ok(error_str) = serde_json::to_string(&error_message) {
                        ctx_addr.do_send(SendText(error_str));
                    }
                }
            }

            // Send completion as proper JSON
            if let Some(addr) = weak_addr.upgrade() {
                let completion = json!({
                    "type": "completion",
                    "message": "Chat message handled"
                });
                if let Ok(completion_str) = serde_json::to_string(&completion) {
                    addr.do_send(SendText(completion_str));
                }
            }
        };

        ctx.spawn(fut.into_actor(self));
    }

    // Handle simulation mode changes
    fn handle_simulation_mode(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, mode: &str) {
        self.simulation_mode = match mode {
            "remote" => {
                info!("Simulation mode set to Remote (GPU-accelerated)");
                // Start GPU position updates when switching to remote mode
                if let Some(_) = &self.state.gpu_compute {
                    self.start_gpu_updates(ctx);
                }
                SimulationMode::Remote
            },
            "gpu" => {
                info!("Simulation mode set to GPU (local)");
                SimulationMode::GPU
            },
            "local" => {
                info!("Simulation mode set to Local (CPU)");
                SimulationMode::Local
            },
            _ => {
                error!("Invalid simulation mode: {}, defaulting to Remote", mode);
                SimulationMode::Remote
            }
        };

        let response = ServerMessage::SimulationModeSet {
            mode: mode.to_string(),
            gpu_enabled: matches!(self.simulation_mode, SimulationMode::Remote | SimulationMode::GPU)
        };
        if let Ok(response_str) = serde_json::to_string(&response) {
            ctx.text(ByteString::from(response_str));
        }
}

    // Handle layout parameter updates and GPU computation
    fn handle_layout(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, params: SimulationParams) {
        let state = self.state.clone();
        let ctx_addr = ctx.address();
        let weak_addr = ctx.address().downgrade();

        let fut = async move {
            if let Some(gpu_compute) = &state.gpu_compute {
                let mut gpu = gpu_compute.write().await;
                
                if let Err(e) = gpu.update_simulation_params(&params) {
                    error!("Failed to update simulation parameters: {}", e);
                    let error_message = ServerMessage::Error {
                        message: format!("Failed to update simulation parameters: {}", e),
                        code: Some("SIMULATION_PARAMS_ERROR".to_string())
                    };
                    if let Ok(error_str) = serde_json::to_string(&error_message) {
                        ctx_addr.do_send(SendText(error_str));
                    }
                    return;
                }

                // Run GPU computation steps
                for _ in 0..params.iterations {
                    if let Err(e) = gpu.step() {
                        error!("GPU compute step failed: {}", e);
                        let error_message = ServerMessage::Error {
                            message: format!("GPU compute step failed: {}", e),
                            code: Some("GPU_COMPUTE_ERROR".to_string())
                        };
                        if let Ok(error_str) = serde_json::to_string(&error_message) {
                            ctx_addr.do_send(SendText(error_str));
                        }
                        return;
                    }
                }

                // Send updated positions
                match gpu.get_node_positions().await {
                    Ok(nodes) => {
                        let binary_data = positions_to_binary(&nodes);
                        ctx_addr.do_send(SendBinary(binary_data));
                    },
                    Err(e) => {
                        error!("Failed to get GPU node positions: {}", e);
                        let error_message = ServerMessage::Error {
                            message: format!("Failed to get GPU node positions: {}", e),
                            code: Some("GPU_POSITION_ERROR".to_string())
                        };
                        if let Ok(error_str) = serde_json::to_string(&error_message) {
                            ctx_addr.do_send(SendText(error_str));
                        }
                    }
                }
            } else {
                error!("GPU compute service not available");
                let error_message = ServerMessage::Error {
                    message: "GPU compute service not available".to_string(),
                    code: Some("GPU_SERVICE_ERROR".to_string())
                };
                if let Ok(error_str) = serde_json::to_string(&error_message) {
                    ctx_addr.do_send(SendText(error_str));
                }
            }

            // Send completion as proper JSON
            if let Some(addr) = weak_addr.upgrade() {
                let completion = json!({
                    "type": "completion",
                    "message": "Layout update complete"
                });
                if let Ok(completion_str) = serde_json::to_string(&completion) {
                    addr.do_send(SendText(completion_str));
                }
            }
        };

        ctx.spawn(fut.into_actor(self));
    }

    // Handle fisheye settings updates
    fn handle_fisheye_settings(&mut self, ctx: &mut WebsocketContext<WebSocketSession>, enabled: bool, strength: f32, focus_point: [f32; 3], radius: f32) {
        let state = self.state.clone();
        let ctx_addr = ctx.address();

        let fut = async move {
            if let Some(gpu_compute) = &state.gpu_compute {
                let mut gpu = gpu_compute.write().await;
                gpu.update_fisheye_params(enabled, strength, focus_point, radius);
                
                // Send updated fisheye settings using ServerMessage enum
                let response = ServerMessage::FisheyeSettingsUpdated {
                    enabled,
                    strength,
                    focus_point,
                    radius,
                };
                if let Ok(response_str) = serde_json::to_string(&response) {
                    ctx_addr.do_send(SendText(response_str));
                }
            } else {
                error!("GPU compute service not available");
                let error_message = ServerMessage::Error {
                    message: "GPU compute service not available".to_string(),
                    code: Some("GPU_SERVICE_ERROR".to_string())
                };
                if let Ok(error_str) = serde_json::to_string(&error_message) {
                    ctx_addr.do_send(SendText(error_str));
                }
            }

            // Send completion
            let completion = json!({
                "type": "completion",
                "message": "Fisheye settings updated"
            });
            if let Ok(completion_str) = serde_json::to_string(&completion) {
                ctx_addr.do_send(SendText(completion_str));
            }
        };

        ctx.spawn(fut.into_actor(self));
    }
}

// Handler implementations for messages
impl Handler<GpuUpdate> for WebSocketSession {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: GpuUpdate, _ctx: &mut Self::Context) -> Self::Result {
        let state = self.state.clone();
        let gpu_compute = if let Some(gpu) = &state.gpu_compute {
            gpu.clone()
        } else {
            return Box::pin(futures::future::ready(()).into_actor(self));
        };

        Box::pin(async move {
            let mut gpu = gpu_compute.write().await;
            if let Err(e) = gpu.step() {
                error!("GPU compute step failed: {}", e);
                return;
}

            // Send binary position updates to all connected clients
            if let Ok(nodes) = gpu.get_node_positions().await {
                // Let WebSocketManager handle the broadcasting
                state.websocket_manager.broadcast_binary(&nodes, false).await;
            }
        }
        .into_actor(self))
    }
}

impl Handler<SendText> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: SendText, ctx: &mut Self::Context) {
        ctx.text(ByteString::from(msg.0));
    }
}

impl Handler<SendBinary> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: SendBinary, ctx: &mut Self::Context) {
        ctx.binary(msg.0);
    }
}

impl Handler<OpenAIMessage> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, msg: OpenAIMessage, _ctx: &mut Self::Context) {
        if let Some(ref ws) = self.openai_ws {
            ws.do_send(msg);
        }
    }
}

impl Handler<OpenAIConnected> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, _: OpenAIConnected, _ctx: &mut Self::Context) {
        debug!("OpenAI WebSocket connected");
    }
}

impl Handler<OpenAIConnectionFailed> for WebSocketSession {
    type Result = ();

    fn handle(&mut self, _: OpenAIConnectionFailed, _ctx: &mut Self::Context) {
        error!("OpenAI WebSocket connection failed");
        self.openai_ws = None;
    }
}
