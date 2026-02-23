//! Database operations

use sqlx::{PgPool, Row};
use sqlx::QueryBuilder;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use crate::models::*;

/// Aggregated event counts returned by `event_stats`.
#[derive(Debug, serde::Serialize)]
pub struct EventStats {
    pub total: i64,
    pub fire_count: i64,
    pub smoke_count: i64,
    pub acknowledged_count: i64,
    pub pending_count: i64,
}

/// Database wrapper
#[derive(Clone)]
pub struct Database {
    pool: PgPool,
    encryption_key: [u8; 32],
}

impl Database {
    /// Create new database wrapper.
    ///
    /// # Panics
    /// The caller (`AppConfig::validate_secrets`) guarantees `encryption_key` is
    /// at least 32 characters before this is called, so the truncation here is
    /// only a safety guard – it should never trigger in production.
    pub fn new(pool: PgPool, encryption_key: String) -> Self {
        let key_bytes = encryption_key.as_bytes();
        // Key must be exactly 32 bytes for AES-256-GCM.
        // config validation ensures length >= 32; we take the first 32 bytes.
        let mut key = [0u8; 32];
        let len = key_bytes.len().min(32);
        key[..len].copy_from_slice(&key_bytes[..len]);

        Self { pool, encryption_key: key }
    }
    
    // ========== Camera Operations ==========
    
    /// List all cameras
    pub async fn list_cameras(&self) -> Result<Vec<Camera>> {
        let rows = sqlx::query_as!(
            CameraRow,
            r#"
            SELECT 
                id, site_id, name, description, detector_camera_id,
                rtsp_url_encrypted, enabled, fps_sample, imgsz, conf_fire, conf_smoke,
                window_size, fire_hits, smoke_hits, cooldown_sec,
                created_at, updated_at
            FROM cameras
            ORDER BY site_id, name
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        rows.into_iter()
            .map(|row| self.row_to_camera(row))
            .collect()
    }
    
    /// Get camera by ID
    pub async fn get_camera(&self, id: &Uuid) -> Result<Option<Camera>> {
        let row = sqlx::query_as!(
            CameraRow,
            r#"
            SELECT 
                id, site_id, name, description, detector_camera_id,
                rtsp_url_encrypted, enabled, fps_sample, imgsz, conf_fire, conf_smoke,
                window_size, fire_hits, smoke_hits, cooldown_sec,
                created_at, updated_at
            FROM cameras
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        
        match row {
            Some(r) => Ok(Some(self.row_to_camera(r)?)),
            None => Ok(None),
        }
    }

    /// Get camera by detector_camera_id (for mapping detector events to API cameras)
    pub async fn get_camera_by_detector_id(&self, detector_camera_id: &str) -> Result<Option<Camera>> {
        let row = sqlx::query_as!(
            CameraRow,
            r#"
            SELECT 
                id, site_id, name, description, detector_camera_id,
                rtsp_url_encrypted, enabled, fps_sample, imgsz, conf_fire, conf_smoke,
                window_size, fire_hits, smoke_hits, cooldown_sec,
                created_at, updated_at
            FROM cameras
            WHERE detector_camera_id = $1
            "#,
            detector_camera_id
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(self.row_to_camera(r)?)),
            None => Ok(None),
        }
    }
    
    /// Create camera
    pub async fn create_camera(&self, input: &CreateCameraInput) -> Result<Camera> {
        let id = Uuid::new_v4();
        let encrypted_url = self.encrypt(&input.rtsp_url)?;
        
        sqlx::query!(
            r#"
            INSERT INTO cameras (
                id, site_id, name, description, detector_camera_id, rtsp_url_encrypted,
                enabled, fps_sample, imgsz, conf_fire, conf_smoke,
                window_size, fire_hits, smoke_hits, cooldown_sec
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
            )
            "#,
            id,
            input.site_id,
            input.name,
            input.description,
            input.detector_camera_id,
            encrypted_url,
            input.enabled.unwrap_or(true),
            input.fps_sample.unwrap_or(3) as i32,
            input.imgsz.unwrap_or(640) as i32,
            input.conf_fire.unwrap_or(0.5),
            input.conf_smoke.unwrap_or(0.4),
            input.window_size.unwrap_or(10) as i32,
            input.fire_hits.unwrap_or(3) as i32,
            input.smoke_hits.unwrap_or(3) as i32,
            input.cooldown_sec.unwrap_or(60) as i32
        )
        .execute(&self.pool)
        .await?;
        
        Ok(self.get_camera(&id).await?.unwrap())
    }
    
    /// Update camera
    pub async fn update_camera(&self, id: &Uuid, input: &UpdateCameraInput) -> Result<Option<Camera>> {
        let existing = match self.get_camera(id).await? {
            Some(c) => c,
            None => return Ok(None),
        };
        
        let encrypted_url = match &input.rtsp_url {
            Some(url) => self.encrypt(url)?,
            None => self.encrypt(&existing.rtsp_url)?,
        };
        
        sqlx::query!(
            r#"
            UPDATE cameras SET
                site_id = COALESCE($2, site_id),
                name = COALESCE($3, name),
                description = COALESCE($4, description),
                detector_camera_id = COALESCE($5, detector_camera_id),
                rtsp_url_encrypted = $6,
                enabled = COALESCE($7, enabled),
                fps_sample = COALESCE($8, fps_sample),
                imgsz = COALESCE($9, imgsz),
                conf_fire = COALESCE($10, conf_fire),
                conf_smoke = COALESCE($11, conf_smoke),
                window_size = COALESCE($12, window_size),
                fire_hits = COALESCE($13, fire_hits),
                smoke_hits = COALESCE($14, smoke_hits),
                cooldown_sec = COALESCE($15, cooldown_sec),
                updated_at = NOW()
            WHERE id = $1
            "#,
            id,
            input.site_id,
            input.name,
            input.description,
            input.detector_camera_id,
            encrypted_url,
            input.enabled,
            input.fps_sample.map(|v| v as i32),
            input.imgsz.map(|v| v as i32),
            input.conf_fire,
            input.conf_smoke,
            input.window_size.map(|v| v as i32),
            input.fire_hits.map(|v| v as i32),
            input.smoke_hits.map(|v| v as i32),
            input.cooldown_sec.map(|v| v as i32)
        )
        .execute(&self.pool)
        .await?;
        
        self.get_camera(id).await
    }
    
    /// Delete camera
    pub async fn delete_camera(&self, id: &Uuid) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM cameras WHERE id = $1", id)
            .execute(&self.pool)
            .await?;
        
        Ok(result.rows_affected() > 0)
    }
    
    // ========== Event Operations ==========

    /// Get a single event by its primary key.
    pub async fn get_event_by_id(&self, id: &Uuid) -> Result<Option<Event>> {
        let row = sqlx::query!(
            r#"
            SELECT
                id, event_type, camera_id, site_id, timestamp,
                confidence, detections, snapshot_path, metadata,
                acknowledged, acknowledged_by, acknowledged_at
            FROM events
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| Event {
            id: r.id,
            event_type: r.event_type,
            camera_id: r.camera_id,
            site_id: r.site_id,
            timestamp: r.timestamp,
            confidence: r.confidence,
            detections: r.detections,
            snapshot_path: r.snapshot_path,
            metadata: r.metadata,
            acknowledged: r.acknowledged,
            acknowledged_by: r.acknowledged_by,
            acknowledged_at: r.acknowledged_at,
        }))
    }

    /// List events with filters and pagination.
    ///
    /// Uses `QueryBuilder` so every dynamic value — including LIMIT and OFFSET —
    /// is passed as a bound parameter ($N), preventing SQL injection.
    pub async fn list_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            r#"SELECT
                id, event_type, camera_id, site_id, timestamp,
                confidence, detections, snapshot_path, metadata,
                acknowledged, acknowledged_by, acknowledged_at
            FROM events
            WHERE 1=1"#,
        );

        if let Some(camera_id) = &filter.camera_id {
            qb.push(" AND camera_id = ").push_bind(*camera_id);
        }
        if let Some(site_id) = &filter.site_id {
            qb.push(" AND site_id = ").push_bind(site_id.as_str());
        }
        if let Some(event_type) = &filter.event_type {
            qb.push(" AND event_type = ").push_bind(event_type.as_str());
        }
        if let Some(start_time) = filter.start_time {
            qb.push(" AND timestamp >= ").push_bind(start_time);
        }
        if let Some(end_time) = filter.end_time {
            qb.push(" AND timestamp <= ").push_bind(end_time);
        }
        if let Some(acknowledged) = filter.acknowledged {
            qb.push(" AND acknowledged = ").push_bind(acknowledged);
        }

        qb.push(" ORDER BY timestamp DESC");

        // Clamp limit/offset to safe ranges – values are i32 from the type
        // system, but we add explicit bounds to prevent absurdly large pages.
        if let Some(limit) = filter.limit {
            let safe_limit = limit.clamp(1, 10_000);
            qb.push(" LIMIT ").push_bind(safe_limit);
        }
        if let Some(offset) = filter.offset {
            let safe_offset = offset.max(0);
            qb.push(" OFFSET ").push_bind(safe_offset);
        }

        let rows = qb.build().fetch_all(&self.pool).await?;

        let events = rows
            .into_iter()
            .map(|row| Event {
                id: row.get("id"),
                event_type: row.get("event_type"),
                camera_id: row.get("camera_id"),
                site_id: row.get("site_id"),
                timestamp: row.get("timestamp"),
                confidence: row.get("confidence"),
                detections: row.get("detections"),
                snapshot_path: row.get("snapshot_path"),
                metadata: row.get("metadata"),
                acknowledged: row.get("acknowledged"),
                acknowledged_by: row.get("acknowledged_by"),
                acknowledged_at: row.get("acknowledged_at"),
            })
            .collect();

        Ok(events)
    }

    /// Aggregate event counts using SQL for efficiency.
    pub async fn event_stats(
        &self,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<EventStats> {
        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "SELECT \
                COUNT(*) AS total, \
                COUNT(*) FILTER (WHERE event_type = 'fire') AS fire_count, \
                COUNT(*) FILTER (WHERE event_type = 'smoke') AS smoke_count, \
                COUNT(*) FILTER (WHERE acknowledged = true) AS acknowledged_count \
             FROM events WHERE 1=1",
        );

        if let Some(st) = start_time {
            qb.push(" AND timestamp >= ").push_bind(st);
        }
        if let Some(et) = end_time {
            qb.push(" AND timestamp <= ").push_bind(et);
        }

        let row = qb.build().fetch_one(&self.pool).await?;

        let total: i64 = row.get("total");
        let fire_count: i64 = row.get("fire_count");
        let smoke_count: i64 = row.get("smoke_count");
        let acknowledged_count: i64 = row.get("acknowledged_count");

        Ok(EventStats {
            total,
            fire_count,
            smoke_count,
            acknowledged_count,
            pending_count: total - acknowledged_count,
        })
    }
    
    /// Save event
    pub async fn save_event(&self, event: &CreateEventInput) -> Result<Event> {
        let id = Uuid::new_v4();
        
        sqlx::query!(
            r#"
            INSERT INTO events (
                id, event_type, camera_id, site_id, timestamp,
                confidence, detections, snapshot_path, metadata
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            id,
            event.event_type,
            event.camera_id,
            event.site_id,
            event.timestamp,
            event.confidence,
            event.detections,
            event.snapshot_path,
            event.metadata
        )
        .execute(&self.pool)
        .await?;
        
        // Return created event
        let row = sqlx::query!(
            r#"
            SELECT * FROM events WHERE id = $1
            "#,
            id
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(Event {
            id: row.id,
            event_type: row.event_type,
            camera_id: row.camera_id,
            site_id: row.site_id,
            timestamp: row.timestamp,
            confidence: row.confidence,
            detections: row.detections,
            snapshot_path: row.snapshot_path,
            metadata: row.metadata,
            acknowledged: row.acknowledged,
            acknowledged_by: row.acknowledged_by,
            acknowledged_at: row.acknowledged_at,
        })
    }
    
    /// Acknowledge event
    pub async fn acknowledge_event(&self, id: &Uuid, user_id: &Uuid) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE events SET
                acknowledged = true,
                acknowledged_by = $2,
                acknowledged_at = NOW()
            WHERE id = $1
            "#,
            id,
            user_id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() > 0)
    }
    
    // ========== User Operations ==========
    
    /// Get user by email
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let row = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, password_hash, name, role, active, telegram_chat_id, created_at, updated_at
            FROM users WHERE email = $1
            "#,
            email
        )
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    /// Get user by ID
    pub async fn get_user_by_id(&self, id: &Uuid) -> Result<Option<User>> {
        let row = sqlx::query_as!(
            User,
            r#"
            SELECT id, email, password_hash, name, role, active, telegram_chat_id, created_at, updated_at
            FROM users WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    // ========== Helper Methods ==========
    
    /// Encrypt RTSP URL
    fn encrypt(&self, plaintext: &str) -> Result<String> {
        let key = Key::<Aes256Gcm>::from_slice(&self.encryption_key);
        let cipher = Aes256Gcm::new(key);
        
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
        
        // Combine nonce + ciphertext
        let mut combined = nonce_bytes.to_vec();
        combined.extend(ciphertext);
        
        Ok(BASE64.encode(&combined))
    }
    
    /// Decrypt RTSP URL
    fn decrypt(&self, encrypted: &str) -> Result<String> {
        let combined = BASE64.decode(encrypted)?;
        
        if combined.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data"));
        }
        
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        
        let key = Key::<Aes256Gcm>::from_slice(&self.encryption_key);
        let cipher = Aes256Gcm::new(key);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
        
        Ok(String::from_utf8(plaintext)?)
    }
    
    /// Convert database row to Camera model
    fn row_to_camera(&self, row: CameraRow) -> Result<Camera> {
        let rtsp_url = self.decrypt(&row.rtsp_url_encrypted)?;
        
        Ok(Camera {
            id: row.id,
            site_id: row.site_id,
            name: row.name,
            description: row.description,
            detector_camera_id: row.detector_camera_id,
            rtsp_url,
            enabled: row.enabled,
            fps_sample: row.fps_sample as u32,
            imgsz: row.imgsz as u32,
            conf_fire: row.conf_fire,
            conf_smoke: row.conf_smoke,
            window_size: row.window_size as u32,
            fire_hits: row.fire_hits as u32,
            smoke_hits: row.smoke_hits as u32,
            cooldown_sec: row.cooldown_sec as u64,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// Internal struct for database row
#[derive(Debug)]
struct CameraRow {
    id: Uuid,
    site_id: String,
    name: String,
    description: Option<String>,
    detector_camera_id: Option<String>,
    rtsp_url_encrypted: String,
    enabled: bool,
    fps_sample: i32,
    imgsz: i32,
    conf_fire: f32,
    conf_smoke: f32,
    window_size: i32,
    fire_hits: i32,
    smoke_hits: i32,
    cooldown_sec: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
