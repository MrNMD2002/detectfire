//! Telegram notification bot

use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use reqwest::Client;
use tracing::{error, info, warn};

use crate::config::TelegramConfig;

/// Telegram bot for notifications
#[derive(Clone)]
pub struct TelegramBot {
    inner: Arc<TelegramBotInner>,
}

struct TelegramBotInner {
    config: TelegramConfig,
    client: Client,
    rate_limiter: RwLock<RateLimiter>,
}

/// Simple rate limiter
struct RateLimiter {
    last_reset: Instant,
    count: u32,
    limit: u32,
}

impl RateLimiter {
    fn new(limit: u32) -> Self {
        Self {
            last_reset: Instant::now(),
            count: 0,
            limit,
        }
    }
    
    fn check(&mut self) -> bool {
        // Reset if minute passed
        if self.last_reset.elapsed() > Duration::from_secs(60) {
            self.count = 0;
            self.last_reset = Instant::now();
        }
        
        if self.count >= self.limit {
            return false;
        }
        
        self.count += 1;
        true
    }
}

impl TelegramBot {
    /// Create new Telegram bot
    pub fn new(config: &TelegramConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            inner: Arc::new(TelegramBotInner {
                config: config.clone(),
                client,
                rate_limiter: RwLock::new(RateLimiter::new(config.rate_limit_per_minute)),
            }),
        }
    }
    
    /// Send a text message
    pub async fn send_message(&self, chat_id: &str, text: &str) -> anyhow::Result<()> {
        if !self.inner.config.enabled {
            return Ok(());
        }
        
        // Check rate limit
        if !self.inner.rate_limiter.write().check() {
            warn!("Telegram rate limit exceeded");
            return Ok(());
        }
        
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.inner.config.bot_token
        );
        
        let response = self.inner.client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
                "parse_mode": "HTML"
            }))
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!(error = %error_text, "Telegram API error");
            return Err(anyhow::anyhow!("Telegram API error: {}", error_text));
        }
        
        info!(chat_id = %chat_id, "Telegram message sent");
        Ok(())
    }
    
    /// Send a photo with caption
    pub async fn send_photo(
        &self,
        chat_id: &str,
        photo: Vec<u8>,
        caption: &str,
    ) -> anyhow::Result<()> {
        if !self.inner.config.enabled {
            return Ok(());
        }
        
        // Check rate limit
        if !self.inner.rate_limiter.write().check() {
            warn!("Telegram rate limit exceeded");
            return Ok(());
        }
        
        let url = format!(
            "https://api.telegram.org/bot{}/sendPhoto",
            self.inner.config.bot_token
        );
        
        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .text("caption", caption.to_string())
            .text("parse_mode", "HTML")
            .part("photo", reqwest::multipart::Part::bytes(photo)
                .file_name("snapshot.jpg")
                .mime_str("image/jpeg")?);
        
        let response = self.inner.client
            .post(&url)
            .multipart(form)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!(error = %error_text, "Telegram API error");
            return Err(anyhow::anyhow!("Telegram API error: {}", error_text));
        }
        
        info!(chat_id = %chat_id, "Telegram photo sent");
        Ok(())
    }
    
    /// Send fire/smoke alert
    pub async fn send_fire_alert(
        &self,
        camera_name: &str,
        site_name: &str,
        event_type: &str,
        confidence: f32,
        snapshot: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let emoji = if event_type == "fire" { "🔥" } else { "💨" };
        let event_name = if event_type == "fire" { "CHÁY" } else { "KHÓI" };
        
        let caption = format!(
            "{} <b>CẢNH BÁO {} PHÁT HIỆN!</b>\n\n\
            📹 Camera: <b>{}</b>\n\
            📍 Vị trí: <b>{}</b>\n\
            🎯 Độ tin cậy: <b>{:.1}%</b>\n\
            🕐 Thời gian: <b>{}</b>\n\n\
            Vui lòng kiểm tra ngay!",
            emoji,
            event_name,
            camera_name,
            site_name,
            confidence * 100.0,
            chrono::Local::now().format("%H:%M:%S %d/%m/%Y")
        );
        
        let chat_id = &self.inner.config.default_chat_id;
        
        match snapshot {
            Some(photo) => self.send_photo(chat_id, photo, &caption).await,
            None => self.send_message(chat_id, &caption).await,
        }
    }
    
    /// Send stream down alert
    pub async fn send_stream_down_alert(
        &self,
        camera_name: &str,
        error: &str,
    ) -> anyhow::Result<()> {
        let text = format!(
            "⚠️ <b>CẢNH BÁO MẤT KẾT NỐI CAMERA</b>\n\n\
            📹 Camera: <b>{}</b>\n\
            ❌ Lỗi: <code>{}</code>\n\
            🕐 Thời gian: <b>{}</b>",
            camera_name,
            error,
            chrono::Local::now().format("%H:%M:%S %d/%m/%Y")
        );
        
        self.send_message(&self.inner.config.default_chat_id, &text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(3);
        
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(!limiter.check()); // Exceeded
    }
}
