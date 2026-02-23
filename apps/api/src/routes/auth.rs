//! Auth routes

use std::sync::Arc;
use axum::{routing::post, Extension, Json, Router};
use validator::Validate;

use crate::{
    auth::{generate_token, verify_password, Claims, AuthUser},
    error::ApiError,
    models::{LoginRequest, LoginResponse, UserInfo},
    AppState,
};

/// Auth routes
pub fn routes() -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/me", axum::routing::get(me))
        .route("/refresh", post(refresh_token))
}

/// Login endpoint
async fn login(
    Extension(state): Extension<Arc<AppState>>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    input.validate()?;
    
    // Find user by email
    let user = state.db.get_user_by_email(&input.email).await?
        .ok_or_else(|| ApiError::AuthError("Invalid email or password".to_string()))?;
    
    // Check if user is active
    if !user.active {
        return Err(ApiError::AuthError("Account is disabled".to_string()));
    }
    
    // Verify password
    let valid = verify_password(&input.password, &user.password_hash)?;
    if !valid {
        return Err(ApiError::AuthError("Invalid email or password".to_string()));
    }
    
    // Generate token
    let expiry_hours = state.config.auth.jwt_expiry_hours;
    let claims = Claims::new(user.id, &user.email, &user.role, expiry_hours);
    let token = generate_token(&claims, &state.config.auth.jwt_secret)?;
    
    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer".to_string(),
        expires_in: (expiry_hours * 3600) as i64,
        user: UserInfo {
            id: user.id,
            email: user.email,
            name: user.name,
            role: user.role,
        },
    }))
}

/// Get current user info
async fn me(
    Extension(state): Extension<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<UserInfo>, ApiError> {
    let db_user = state.db.get_user_by_id(&user.id).await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
    
    Ok(Json(UserInfo {
        id: db_user.id,
        email: db_user.email,
        name: db_user.name,
        role: db_user.role,
    }))
}

/// Refresh token
async fn refresh_token(
    Extension(state): Extension<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<LoginResponse>, ApiError> {
    let db_user = state.db.get_user_by_id(&user.id).await?
        .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;
    
    let expiry_hours = state.config.auth.jwt_expiry_hours;
    let claims = Claims::new(db_user.id, &db_user.email, &db_user.role, expiry_hours);
    let token = generate_token(&claims, &state.config.auth.jwt_secret)?;
    
    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer".to_string(),
        expires_in: (expiry_hours * 3600) as i64,
        user: UserInfo {
            id: db_user.id,
            email: db_user.email,
            name: db_user.name,
            role: db_user.role,
        },
    }))
}
