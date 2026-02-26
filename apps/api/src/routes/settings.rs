//! Settings routes
//!
//! GET  /api/settings/telegram       — current config (bot_token masked)
//! PUT  /api/settings/telegram       — update config (persists to DB)
//! POST /api/settings/telegram/test  — send test message

use std::sync::Arc;
use axum::{
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::{auth::AuthUser, config::TelegramConfig, error::ApiError, AppState};

pub fn routes() -> Router {
    Router::new()
        .route("/telegram", get(get_telegram_settings))
        .route("/telegram", put(update_telegram_settings))
        .route("/telegram/test", post(test_telegram))
}

// ── Response / request types ─────────────────────────────────────────────────

#[derive(Serialize)]
struct TelegramSettingsResponse {
    enabled: bool,
    /// Last 6 chars visible, rest masked
    bot_token_masked: String,
    default_chat_id: String,
    rate_limit_per_minute: u32,
}

#[derive(Deserialize)]
struct UpdateTelegramRequest {
    /// New bot token. Empty string / omitted = keep the existing token.
    bot_token: Option<String>,
    default_chat_id: String,
    enabled: bool,
    /// Optional override; defaults to current value if omitted.
    rate_limit_per_minute: Option<u32>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/settings/telegram — read current config (token masked)
async fn get_telegram_settings(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<TelegramSettingsResponse>, ApiError> {
    let cfg = state.telegram.current_config();

    let bot_token_masked = if cfg.bot_token.len() > 6 {
        format!("••••••{}", &cfg.bot_token[cfg.bot_token.len() - 6..])
    } else if cfg.bot_token.is_empty() {
        "(chưa cấu hình)".to_string()
    } else {
        "••••••".to_string()
    };

    Ok(Json(TelegramSettingsResponse {
        enabled: cfg.enabled,
        bot_token_masked,
        default_chat_id: cfg.default_chat_id,
        rate_limit_per_minute: cfg.rate_limit_per_minute,
    }))
}

/// PUT /api/settings/telegram — update config and persist to DB
async fn update_telegram_settings(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
    Json(body): Json<UpdateTelegramRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let current = state.telegram.current_config();

    // Use new bot_token only if provided and non-empty; else keep current
    let bot_token = body
        .bot_token
        .filter(|s| !s.is_empty())
        .unwrap_or(current.bot_token.clone());

    let rate_limit = body
        .rate_limit_per_minute
        .unwrap_or(current.rate_limit_per_minute);

    let new_config = TelegramConfig {
        enabled: body.enabled,
        bot_token: bot_token.clone(),
        default_chat_id: body.default_chat_id.clone(),
        rate_limit_per_minute: rate_limit,
    };

    // Persist each field to system_settings table
    state
        .db
        .set_setting("telegram.bot_token", &bot_token)
        .await
        .map_err(|e| ApiError::InternalError(format!("DB error: {}", e)))?;
    state
        .db
        .set_setting("telegram.chat_id", &body.default_chat_id)
        .await
        .map_err(|e| ApiError::InternalError(format!("DB error: {}", e)))?;
    state
        .db
        .set_setting(
            "telegram.enabled",
            if body.enabled { "true" } else { "false" },
        )
        .await
        .map_err(|e| ApiError::InternalError(format!("DB error: {}", e)))?;
    state
        .db
        .set_setting("telegram.rate_limit", &rate_limit.to_string())
        .await
        .map_err(|e| ApiError::InternalError(format!("DB error: {}", e)))?;

    // Apply immediately — no restart needed
    state.telegram.update_config(new_config);

    Ok(Json(serde_json::json!({ "ok": true, "message": "Telegram settings updated" })))
}

/// POST /api/settings/telegram/test — send a test message to the configured chat
async fn test_telegram(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cfg = state.telegram.current_config();

    if !cfg.enabled {
        return Err(ApiError::BadRequest(
            "Telegram chưa được bật. Bật lên trong phần cài đặt trước.".to_string(),
        ));
    }

    if cfg.bot_token.is_empty() {
        return Err(ApiError::BadRequest("Bot Token chưa được cấu hình.".to_string()));
    }

    if cfg.default_chat_id.is_empty() {
        return Err(ApiError::BadRequest("Chat ID chưa được cấu hình.".to_string()));
    }

    let msg = format!(
        "✅ <b>Test kết nối thành công!</b>\n\
        🕐 Thời gian: <b>{}</b>\n\
        📡 Hệ thống phát hiện cháy khói đang hoạt động bình thường.",
        chrono::Local::now().format("%H:%M:%S %d/%m/%Y")
    );

    state
        .telegram
        .send_message(&cfg.default_chat_id, &msg)
        .await
        .map_err(|e| ApiError::InternalError(format!("Telegram error: {}", e)))?;

    Ok(Json(serde_json::json!({ "ok": true, "message": "Test message sent" })))
}
