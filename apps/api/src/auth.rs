//! JWT authentication

use std::sync::Arc;
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
    Extension,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, AppState};

/// JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // User ID
    pub email: String,
    pub role: String,
    pub exp: i64,     // Expiration
    pub iat: i64,     // Issued at
}

impl Claims {
    /// Create new claims for a user
    pub fn new(user_id: Uuid, email: &str, role: &str, expiry_hours: u64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(expiry_hours as i64);
        
        Self {
            sub: user_id.to_string(),
            email: email.to_string(),
            role: role.to_string(),
            exp: exp.timestamp(),
            iat: now.timestamp(),
        }
    }
    
    /// Get user ID
    pub fn user_id(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.sub).ok()
    }
}

/// Generate JWT token
pub fn generate_token(claims: &Claims, secret: &str) -> Result<String, ApiError> {
    let key = EncodingKey::from_secret(secret.as_bytes());
    
    encode(&Header::default(), claims, &key)
        .map_err(|e| ApiError::InternalError(format!("Token generation failed: {}", e)))
}

/// Verify JWT token
pub fn verify_token(token: &str, secret: &str) -> Result<TokenData<Claims>, ApiError> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::default();
    
    decode::<Claims>(token, &key, &validation)
        .map_err(|e| ApiError::AuthError(format!("Invalid token: {}", e)))
}

/// Authenticated user extractor
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub role: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;
    
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Get state
        let Extension(app_state) = Extension::<Arc<AppState>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::InternalError("Failed to get app state".to_string()))?;
        
        // Get Authorization header
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::AuthError("Missing Authorization header".to_string()))?;
        
        // Extract Bearer token
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::AuthError("Invalid Authorization header format".to_string()))?;
        
        // Verify token
        let token_data = verify_token(token, &app_state.config.auth.jwt_secret)?;
        let claims = token_data.claims;
        
        // Check expiration
        let now = Utc::now().timestamp();
        if claims.exp < now {
            return Err(ApiError::AuthError("Token expired".to_string()));
        }
        
        // Get user ID
        let user_id = claims.user_id()
            .ok_or_else(|| ApiError::AuthError("Invalid user ID in token".to_string()))?;
        
        Ok(AuthUser {
            id: user_id,
            email: claims.email,
            role: claims.role,
        })
    }
}

/// Admin user extractor (requires admin role)
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

#[async_trait]
impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;
    
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        
        if user.role != "admin" {
            return Err(ApiError::Forbidden("Admin access required".to_string()));
        }
        
        Ok(AdminUser(user))
    }
}

/// Hash password
pub fn hash_password(password: &str, cost: u32) -> Result<String, ApiError> {
    bcrypt::hash(password, cost)
        .map_err(|e| ApiError::InternalError(format!("Password hashing failed: {}", e)))
}

/// Verify password
pub fn verify_password(password: &str, hash: &str) -> Result<bool, ApiError> {
    bcrypt::verify(password, hash)
        .map_err(|e| ApiError::InternalError(format!("Password verification failed: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify_token() {
        let claims = Claims::new(Uuid::new_v4(), "test@example.com", "user", 24);
        let secret = "test-secret-key";
        
        let token = generate_token(&claims, secret).unwrap();
        let verified = verify_token(&token, secret).unwrap();
        
        assert_eq!(verified.claims.email, "test@example.com");
        assert_eq!(verified.claims.role, "user");
    }

    #[test]
    fn test_password_hash_and_verify() {
        let password = "my-secure-password";
        let hash = hash_password(password, 4).unwrap();
        
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("wrong-password", &hash).unwrap());
    }
}
