//! Settings routes — GET current config (masked), POST test Telegram

use std::sync::Arc;
use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Serialize;

use crate::{auth::AuthUser, error::ApiError, AppState};

pub fn routes() -> Router {
    Router::new()
        .route("/telegram", get(get_telegram_settings))
        .route("/telegram/test", post(test_telegram))
}

#[derive(Serialize)]
struct TelegramSettingsResponse {
    enabled: bool,
    /// Masked bot token — show only last 6 chars
    bot_token_masked: String,
    default_chat_id: String,
    rate_limit_per_minute: u32,
}

/// GET /api/settings/telegram — returns current config (token masked)
async fn get_telegram_settings(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<TelegramSettingsResponse>, ApiError> {
    let cfg = &state.config.telegram;

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
        default_chat_id: cfg.default_chat_id.clone(),
        rate_limit_per_minute: cfg.rate_limit_per_minute,
    }))
}

/// POST /api/settings/telegram/test — sends a test message to the configured chat
async fn test_telegram(
    Extension(state): Extension<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.config.telegram.enabled {
        return Err(ApiError::BadRequest(
            "Telegram chưa được bật. Đặt FIRE_DETECT__TELEGRAM__ENABLED=true".to_string(),
        ));
    }

    let msg = format!(
        "✅ <b>Test kết nối thành công!</b>\n\
        🕐 Thời gian: <b>{}</b>\n\
        📡 Hệ thống phát hiện cháy khói đang hoạt động bình thường.",
        chrono::Local::now().format("%H:%M:%S %d/%m/%Y")
    );

    state
        .telegram
        .send_message(&state.config.telegram.default_chat_id, &msg)
        .await
        .map_err(|e| ApiError::InternalError(format!("Telegram error: {}", e)))?;

    Ok(Json(serde_json::json!({ "ok": true, "message": "Test message sent" })))
}
