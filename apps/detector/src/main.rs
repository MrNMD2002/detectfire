//! # Fire & Smoke Detector Service
//! 
//! Multi-camera RTSP fire/smoke detection service using YOLOv10 with ONNX Runtime.
//! 
//! ## Architecture
//! 
//! ```text
//! RTSP Cameras → GStreamer Ingest → Frame Sampler → Queue(1) → Inference → Decision Engine → Events
//! ```
//! 
//! ## Modules
//! 
//! - `config`: Configuration loading and validation
//! - `camera`: Camera ingestion and frame sampling
//! - `inference`: ONNX Runtime YOLOv10 inference
//! - `decision`: Sliding window decision engine with cooldown
//! - `event`: Event publishing to API service

mod config;
mod camera;
mod inference;
mod decision;
mod event;
mod error;
mod metrics;
mod stream;

use std::sync::Arc;
use anyhow::Result;
use tokio::signal;
use tracing::{info, error};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::AppConfig;
use crate::camera::CameraManager;
use crate::inference::InferenceEngine;
use crate::decision::DecisionEngine;
use crate::event::EventPublisher;
use crate::metrics::MetricsServer;

/// Application state shared across all components
pub struct AppState {
    pub config: AppConfig,
    pub camera_manager: CameraManager,
    pub inference_engine: InferenceEngine,
    pub decision_engine: DecisionEngine,
    pub event_publisher: EventPublisher,
}

fn main() {
    // Log to stderr immediately so Docker captures output before any exit
    eprintln!("[detector] Starting...");
    
    // Check model file exists BEFORE any async - fail fast with clear message
    let model_path = std::path::Path::new("/app/models/best.onnx");
    let model_path_alt = std::path::Path::new("models/best.onnx");
    eprintln!("[detector] Checking model: {} (exists={}), alt {} (exists={})",
        model_path.display(), model_path.exists(),
        model_path_alt.display(), model_path_alt.exists());
    if !model_path.exists() && !model_path_alt.exists() {
        eprintln!("ERROR: Model file not found: {}", model_path.display());
        eprintln!("Detector requires best.onnx in models/ folder.");
        eprintln!("Export: cd models && python export_onnx.py --weights best.pt --output best.onnx");
        eprintln!("Or run: .\\models\\export_and_validate.ps1");
        std::process::exit(1);
    }

    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {:?}", info);
    }));

    eprintln!("[detector] Creating tokio runtime...");
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    eprintln!("[detector] Running async_main...");
    if let Err(e) = rt.block_on(async_main()) {
        eprintln!("FATAL: {}", e);
        std::process::exit(1);
    }
    eprintln!("[detector] Exited normally (unexpected)");
}

async fn async_main() -> Result<()> {
    init_logging()?;
    info!("Starting Fire & Smoke Detector Service v{}", env!("CARGO_PKG_VERSION"));

    let config = AppConfig::load()?;
    info!(
        cameras = config.cameras.len(),
        device = %config.global.inference.device,
        "Configuration loaded"
    );

    let inference_engine = InferenceEngine::new(&config.global.inference)?;

    // Initialize decision engine
    let decision_engine = DecisionEngine::new();

    // Initialize event publisher
    let event_publisher = EventPublisher::new(&config.global)?;
    
    let camera_manager = CameraManager::new(
        config.cameras.clone(),
        inference_engine.clone(),
        decision_engine.clone(),
        event_publisher.clone(),
    )?;

    // Create shared application state
    let app_state = Arc::new(AppState {
        config: config.clone(),
        camera_manager,
        inference_engine,
        decision_engine,
        event_publisher,
    });

    // Start metrics server
    let metrics_handle = if config.global.monitoring.metrics.enabled {
        Some(tokio::spawn(MetricsServer::run(
            config.global.monitoring.metrics.clone(),
        )))
    } else {
        None
    };

    // Start camera pipelines
    app_state.camera_manager.start_all().await?;

    // Start gRPC server for API communication
    let grpc_port = config.global.server.detector.grpc_port;
    let grpc_handle = tokio::spawn({
        let state = app_state.clone();
        async move {
            if let Err(e) = start_grpc_server(state, grpc_port).await {
                error!(error = %e, "gRPC server failed");
            }
        }
    });

    // Start MJPEG stream HTTP server (push model, port = grpc_port + 1000)
    let stream_port = config.global.server.detector.grpc_port + 1000;
    let stream_handle = tokio::spawn({
        let state = app_state.clone();
        let port = stream_port;
        async move {
            if let Err(e) = start_stream_server(state, port).await {
                error!(error = %e, "Stream server failed");
            }
        }
    });
    info!(port = stream_port, "MJPEG stream server started");

    // Wait for shutdown signal
    info!("Detector service running. Press Ctrl+C to stop.");
    wait_for_shutdown().await;

    // Graceful shutdown
    info!("Shutting down...");
    app_state.camera_manager.stop_all().await;

    if let Some(handle) = metrics_handle {
        handle.abort();
    }
    grpc_handle.abort();
    stream_handle.abort();

    info!("Detector service stopped");
    Ok(())
}

/// Initialize structured JSON logging
fn init_logging() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // Check if JSON format is requested
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

/// Start HLS stream HTTP server
async fn start_stream_server(state: Arc<AppState>, port: u16) -> Result<()> {
    use axum::Router;
    use std::net::SocketAddr;
    use tower::ServiceBuilder;
    use tower_http::cors::{Any, CorsLayer};

    let stream_server = stream::StreamServer::new(state);
    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
        .merge(stream_server.router())
        .layer(
            ServiceBuilder::new().layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            ),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Start gRPC server for detector service
async fn start_grpc_server(state: Arc<AppState>, port: u16) -> Result<()> {
    use tonic::transport::Server;
    use crate::event::detector_grpc::{proto, DetectorGrpcService};

    let addr = format!("0.0.0.0:{}", port).parse()?;
    let service = DetectorGrpcService::new(state);

    info!(port = port, "Starting gRPC server");

    Server::builder()
        .add_service(proto::detector_service_server::DetectorServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn wait_for_shutdown() {
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
        _ = ctrl_c => {
            info!("Received Ctrl+C");
        }
        _ = terminate => {
            info!("Received SIGTERM");
        }
    }
}
