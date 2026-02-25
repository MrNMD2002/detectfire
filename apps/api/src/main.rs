//! # Fire & Smoke Detection API Service
//!
//! REST API service providing:
//! - Camera CRUD operations
//! - Event management and WebSocket streaming
//! - Telegram notification integration
//! - JWT-based authentication

mod config;
mod db;
mod routes;
mod auth;
mod telegram;
mod ws;
mod error;
mod models;
mod detector_client;
mod camera_sync;

use std::sync::Arc;
use std::net::SocketAddr;
use anyhow::Result;
use axum::{Router, Extension};
use axum::http::HeaderValue;
use reqwest::Client;
use sqlx::postgres::PgPoolOptions;
use tokio::signal;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tower_http::compression::CompressionLayer;
use tracing::{error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::AppConfig;
use crate::db::Database;
use crate::telegram::TelegramBot;
use crate::ws::EventBroadcaster;

/// Application state shared across all handlers
pub struct AppState {
    pub config: AppConfig,
    pub db: Database,
    pub telegram: TelegramBot,
    pub event_broadcaster: EventBroadcaster,
    /// Shared HTTP client – reuse across requests instead of creating per-request
    pub http_client: Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    init_logging()?;

    info!("Starting Fire & Smoke Detection API v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = AppConfig::load()?;
    info!(
        host = %config.server.host,
        port = config.server.port,
        "Configuration loaded"
    );

    // Initialize database
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .connect(&config.database.url())
        .await?;
    
    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;
    
    info!("Database connected and migrations applied");

    let db = Database::new(pool, config.database.encryption_key.clone());

    // Initialize Telegram bot
    let telegram = TelegramBot::new(&config.telegram);

    // Initialize event broadcaster
    let event_broadcaster = EventBroadcaster::new();

    // Shared HTTP client (reused across all stream proxy requests)
    let http_client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Create shared state
    let state = Arc::new(AppState {
        config: config.clone(),
        db,
        telegram,
        event_broadcaster,
        http_client,
    });

    // Build router
    let app = build_router(state.clone());

    // Start server
    let addr = SocketAddr::from((
        config.server.host.parse::<std::net::IpAddr>()?,
        config.server.port,
    ));
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API server listening on http://{}", addr);

    // Write initial cameras.yaml from DB so detector sees the same config as the web UI
    match state.db.list_cameras().await {
        Ok(cameras) => {
            if let Err(e) = camera_sync::write_cameras_yaml(&cameras) {
                tracing::warn!(error = %e, "Failed to write initial cameras.yaml");
            }
        }
        Err(e) => tracing::warn!(error = %e, "Failed to list cameras for initial sync"),
    }

    // Start event listener from detector (background task)
    let event_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_event_listener(event_state).await {
            tracing::error!(error = %e, "Event listener failed");
        }
    });
    
    info!("API service ready");

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("API server stopped");
    Ok(())
}

/// Build the Axum router with all routes
fn build_router(state: Arc<AppState>) -> Router {
    // Build CORS layer with explicit allowed origins from config.
    // An empty list means no CORS headers are emitted (correct for same-origin
    // nginx deployments). Wildcard `Any` is intentionally avoided.
    let cors = {
        let allowed: Vec<HeaderValue> = state
            .config
            .server
            .cors_origins
            .iter()
            .filter_map(|o| o.parse::<HeaderValue>().ok())
            .collect();

        if allowed.is_empty() {
            // No origins configured → disable CORS (safe default)
            CorsLayer::new()
        } else {
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(allowed))
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::ACCEPT,
                ])
        }
    };

    Router::new()
        // API routes
        .nest("/api", routes::api_routes())
        // WebSocket route
        .nest("/ws", routes::ws_routes())
        // Health check
        .route("/health", axum::routing::get(routes::health::health_check))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(Extension(state))
}

/// Initialize structured logging
fn init_logging() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let json_logging = std::env::var("LOG_FORMAT")
        .map(|v| v.to_lowercase() == "json")
        .unwrap_or(true);

    if json_logging {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json().flatten_event(true))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().pretty())
            .init();
    }

    Ok(())
}

/// Start listening for events from detector service
async fn start_event_listener(state: Arc<AppState>) -> Result<()> {
    use crate::detector_client::DetectorClient;
    
    // Connect to detector gRPC service
    let detector_addr = format!(
        "{}:{}",
        state.config.detector.host,
        state.config.detector.grpc_port
    );
    
    info!(addr = %detector_addr, "Connecting to detector service");
    
    // Retry connection with exponential backoff
    let mut retry_delay = 1u64;
    let max_retry_delay = 60u64;
    
    loop {
        match DetectorClient::connect(&detector_addr).await {
            Ok(mut client) => {
                info!("Connected to detector service, starting event stream");
                retry_delay = 1; // Reset retry delay on success
                
                // Stream events - this will run until connection is lost
                if let Err(e) = client.stream_events(state.clone()).await {
                    error!(error = %e, "Event stream error, will retry");
                }
            }
            Err(e) => {
                error!(
                    error = %e,
                    retry_in_secs = retry_delay,
                    "Failed to connect to detector service, retrying"
                );
            }
        }
        
        // Exponential backoff before retry
        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
        retry_delay = (retry_delay * 2).min(max_retry_delay);
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C"),
        _ = terminate => info!("Received SIGTERM"),
    }
}
