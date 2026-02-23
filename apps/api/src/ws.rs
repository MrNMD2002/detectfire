//! WebSocket event streaming

use std::sync::Arc;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, error, info};

use crate::AppState;

/// Event broadcaster for WebSocket clients
#[derive(Clone)]
pub struct EventBroadcaster {
    tx: broadcast::Sender<String>,
}

/// Default broadcast channel capacity – increase if clients lag behind producers.
/// Set FIRE_DETECT__SERVER__WS_BROADCAST_CAPACITY to override.
const DEFAULT_BROADCAST_CAPACITY: usize = 1_024;

impl EventBroadcaster {
    /// Create new broadcaster with configurable channel capacity.
    pub fn new() -> Self {
        let capacity = std::env::var("FIRE_DETECT__SERVER__WS_BROADCAST_CAPACITY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_BROADCAST_CAPACITY);

        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }
    
    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
    
    /// Broadcast an event
    pub fn broadcast(&self, event: String) {
        if let Err(e) = self.tx.send(event) {
            debug!(error = %e, "No WebSocket clients connected");
        }
    }
    
    /// Get number of subscribers
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    
    info!("WebSocket client connected");
    
    // Subscribe to events
    let mut rx = state.event_broadcaster.subscribe();
    
    // Spawn task to send events to client
    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if sender.send(Message::Text(event)).await.is_err() {
                break;
            }
        }
    });
    
    // Handle incoming messages (ping/pong, etc.)
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Ping(_data)) => {
                    debug!("Received ping");
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket client disconnected");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "WebSocket error");
                    break;
                }
                _ => {}
            }
        }
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
    
    info!("WebSocket connection closed");
}
