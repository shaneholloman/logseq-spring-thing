use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use tungstenite::protocol::Message;
use tungstenite::http::Request;
use serde_json::json;
use std::sync::Arc;
use tokio::task;
use crate::config::Settings;
use log::{info, error, debug};
use futures::{SinkExt, StreamExt};
use std::error::Error;
use tokio::net::TcpStream;
use url::Url;
use std::process::{Command, Stdio};
use std::io::Write;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use crate::types::speech::{SpeechError, SpeechCommand, TTSProvider};

pub struct SpeechService {
    sender: Arc<Mutex<mpsc::Sender<SpeechCommand>>>,
    settings: Arc<RwLock<Settings>>,
    tts_provider: Arc<RwLock<TTSProvider>>,
}

impl SpeechService {
    pub fn new(settings: Arc<RwLock<Settings>>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let sender = Arc::new(Mutex::new(tx));

        let service = SpeechService {
            sender,
            settings,
            tts_provider: Arc::new(RwLock::new(TTSProvider::Sonata)),
        };

        service.start(rx);
        service
    }

    fn start(&self, mut receiver: mpsc::Receiver<SpeechCommand>) {
        let settings = Arc::clone(&self.settings);
        let tts_provider = Arc::clone(&self.tts_provider);

        task::spawn(async move {
            let mut ws_stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>> = None;

            while let Some(command) = receiver.recv().await {
                match command {
                    SpeechCommand::Initialize => {
                        let current_provider = tts_provider.read().await;
                        if let TTSProvider::OpenAI = *current_provider {
                            let settings = settings.read().await;
                            
                            let url = format!(
                                "wss://api.openai.com/v1/realtime?model=gpt-4o-realtime-preview-2024-10-01"
                            );
                            let url = match Url::parse(&url) {
                                Ok(url) => url,
                                Err(e) => {
                                    error!("Failed to parse OpenAI URL: {}", e);
                                    continue;
                                }
                            };
                            
                            let request = match Request::builder()
                                .uri(url.as_str())
                                .header("Authorization", format!("Bearer {}", settings.openai.api_key))
                                .header("OpenAI-Beta", "realtime=v1")
                                .header("Content-Type", "application/json")
                                .header("User-Agent", "WebXR Graph")
                                .header("Sec-WebSocket-Version", "13")
                                .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
                                .header("Connection", "Upgrade")
                                .header("Upgrade", "websocket")
                                .body(()) {
                                    Ok(req) => req,
                                    Err(e) => {
                                        error!("Failed to build request: {}", e);
                                        continue;
                                    }
                                };

                            match connect_async(request).await {
                                Ok((mut stream, _)) => {
                                    info!("Connected to OpenAI Realtime API");
                                    
                                    let init_event = json!({
                                        "type": "response.create",
                                        "response": {
                                            "modalities": ["text", "audio"],
                                            "instructions": "You are a helpful AI assistant. Respond naturally and conversationally."
                                        }
                                    });
                                    
                                    if let Err(e) = stream.send(Message::Text(init_event.to_string())).await {
                                        error!("Failed to send initial response.create event: {}", e);
                                        continue;
                                    }
                                    
                                    ws_stream = Some(stream);
                                },
                                Err(e) => error!("Failed to connect to OpenAI Realtime API: {}", e),
                            }
                        }
                    },
                    SpeechCommand::SendMessage(msg) => {
                        let current_provider = tts_provider.read().await;
                        match *current_provider {
                            TTSProvider::OpenAI => {
                                if let Some(stream) = &mut ws_stream {
                                    let msg_event = json!({
                                        "type": "conversation.item.create",
                                        "item": {
                                            "type": "message",
                                            "role": "user",
                                            "content": [{
                                                "type": "input_text",
                                                "text": msg
                                            }]
                                        }
                                    });

                                    if let Err(e) = stream.send(Message::Text(msg_event.to_string())).await {
                                        error!("Failed to send message to OpenAI: {}", e);
                                        continue;
                                    }

                                    let response_event = json!({
                                        "type": "response.create"
                                    });
                                    
                                    if let Err(e) = stream.send(Message::Text(response_event.to_string())).await {
                                        error!("Failed to request response from OpenAI: {}", e);
                                        continue;
                                    }
                                    
                                    while let Some(message) = stream.next().await {
                                        match message {
                                            Ok(Message::Text(text)) => {
                                                let event = match serde_json::from_str::<serde_json::Value>(&text) {
                                                    Ok(event) => event,
                                                    Err(e) => {
                                                        error!("Failed to parse server event: {}", e);
                                                        continue;
                                                    }
                                                };
                                                
                                                match event["type"].as_str() {
                                                    Some("conversation.item.created") => {
                                                        if let Some(content) = event["item"]["content"].as_array() {
                                                            for item in content {
                                                                if item["type"] == "audio" {
                                                                    if let Some(audio_data) = item["audio"].as_str() {
                                                                        match BASE64.decode(audio_data) {
                                                                            Ok(audio_bytes) => {
                                                                                // Note: Audio data will be handled by socket-flow server
                                                                                debug!("Received audio data of size: {}", audio_bytes.len());
                                                                            },
                                                                            Err(e) => error!("Failed to decode audio data: {}", e),
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    },
                                                    Some("error") => {
                                                        error!("OpenAI Realtime API error: {:?}", event);
                                                        break;
                                                    },
                                                    Some("response.completed") => break,
                                                    _ => {}
                                                }
                                            },
                                            Ok(Message::Close(_)) => break,
                                            Err(e) => {
                                                error!("Error receiving from OpenAI: {}", e);
                                                break;
                                            },
                                            _ => {}
                                        }
                                    }
                                } else {
                                    error!("OpenAI WebSocket not initialized");
                                }
                            },
                            TTSProvider::Sonata => {
                                let mut child = match Command::new("python3")
                                    .arg("src/generate_audio.py")
                                    .stdin(Stdio::piped())
                                    .stdout(Stdio::piped())
                                    .spawn() {
                                        Ok(child) => child,
                                        Err(e) => {
                                            error!("Failed to spawn Python process: {}", e);
                                            continue;
                                        }
                                    };

                                if let Some(mut stdin) = child.stdin.take() {
                                    if let Err(e) = stdin.write_all(msg.as_bytes()) {
                                        error!("Failed to write to stdin: {}", e);
                                        continue;
                                    }
                                    drop(stdin);
                                }

                                match child.wait_with_output() {
                                    Ok(output) => {
                                        if output.status.success() {
                                            // Note: Audio data will be handled by socket-flow server
                                            debug!("Generated audio data of size: {}", output.stdout.len());
                                        } else {
                                            error!("Sonata TTS failed: {}", String::from_utf8_lossy(&output.stderr));
                                        }
                                    },
                                    Err(e) => error!("Failed to get child process output: {}", e),
                                }
                            }
                        }
                    },
                    SpeechCommand::Close => {
                        if let Some(mut stream) = ws_stream.take() {
                            if let Err(e) = stream.send(Message::Close(None)).await {
                                error!("Failed to send close frame: {}", e);
                            }
                        }
                        break;
                    },
                    SpeechCommand::SetTTSProvider(new_provider) => {
                        let mut provider = tts_provider.write().await;
                        *provider = new_provider;
                        info!("TTS provider set to: {:?}", *provider);
                    }
                }
            }
        });
    }

    pub async fn initialize(&self) -> Result<(), Box<dyn Error>> {
        let command = SpeechCommand::Initialize;
        self.sender.lock().await.send(command).await.map_err(|e| Box::new(SpeechError::from(e)))?;
        Ok(())
    }

    pub async fn send_message(&self, message: String) -> Result<(), Box<dyn Error>> {
        let command = SpeechCommand::SendMessage(message);
        self.sender.lock().await.send(command).await.map_err(|e| Box::new(SpeechError::from(e)))?;
        Ok(())
    }

    pub async fn close(&self) -> Result<(), Box<dyn Error>> {
        let command = SpeechCommand::Close;
        self.sender.lock().await.send(command).await.map_err(|e| Box::new(SpeechError::from(e)))?;
        Ok(())
    }

    pub async fn set_tts_provider(&self, use_openai: bool) -> Result<(), Box<dyn Error>> {
        let provider = if use_openai {
            TTSProvider::OpenAI
        } else {
            TTSProvider::Sonata
        };
        let command = SpeechCommand::SetTTSProvider(provider);
        self.sender.lock().await.send(command).await.map_err(|e| Box::new(SpeechError::from(e)))?;
        Ok(())
    }
}
