use axum::{
    extract::{Request, State},
    http::{self, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{AuthResponse, ErrorResponse, LoginRequest, RegisterRequest, User, UserPublic};

const JWT_SECRET: &str = "jamhub-secret-change-in-production";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: usize,
}

/// Extension-based extractor: handlers receive `AuthUser(user_id)`.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser(pub Uuid);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AuthUser {
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Uuid>()
            .copied()
            .map(AuthUser)
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorResponse {
                        error: "Authentication required".into(),
                    }),
                )
            })
    }
}

/// Optional auth extractor: returns Some(user_id) if authenticated, None otherwise.
#[derive(Debug, Clone, Copy)]
pub struct OptionalAuthUser(pub Option<Uuid>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for OptionalAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalAuthUser(parts.extensions.get::<Uuid>().copied()))
    }
}

/// Middleware: validates JWT and inserts user_id into request extensions.
pub async fn jwt_auth(mut req: Request, next: Next) -> Response {
    let auth_header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if let Some(header) = auth_header {
        let token = header.strip_prefix("Bearer ").unwrap_or(&header);
        if let Ok(data) = decode::<Claims>(
            token,
            &DecodingKey::from_secret(JWT_SECRET.as_bytes()),
            &Validation::default(),
        ) {
            req.extensions_mut().insert(data.claims.sub);
        }
    }

    next.run(req).await
}

/// Helper type for JSON error responses from auth handlers.
type AuthError = (StatusCode, Json<ErrorResponse>);

fn err(status: StatusCode, msg: &str) -> AuthError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

fn make_token(user_id: Uuid) -> Result<String, AuthError> {
    let exp = (Utc::now() + chrono::Duration::days(30)).timestamp() as usize;
    let claims = Claims { sub: user_id, exp };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token"))
}

// ── Handlers ──

async fn register(
    State(pool): State<PgPool>,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if body.username.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Username is required"));
    }
    if body.email.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Email is required"));
    }
    if body.password.len() < 4 {
        return Err(err(StatusCode::BAD_REQUEST, "Password too short (min 4 chars)"));
    }

    println!("[AUTH] Register: username={}, email={}", body.username, body.email);

    let hash = bcrypt::hash(&body.password, bcrypt::DEFAULT_COST)
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password"))?;

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(&body.username)
    .bind(&body.email)
    .bind(&hash)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("register error: {e}");
        err(StatusCode::CONFLICT, "Username or email already exists")
    })?;

    let token = make_token(user.id)?;

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            token,
            user: user.into(),
        }),
    ))
}

async fn login(
    State(pool): State<PgPool>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    println!("[AUTH] Login attempt: email={}", body.email);

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&body.email)
        .fetch_optional(&pool)
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "Invalid email or password"))?;

    let valid =
        bcrypt::verify(&body.password, &user.password_hash)
            .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Password verification failed"))?;
    if !valid {
        return Err(err(StatusCode::UNAUTHORIZED, "Invalid email or password"));
    }

    let token = make_token(user.id)?;

    println!("[AUTH] Login success: user_id={}", user.id);

    Ok(Json(AuthResponse {
        token,
        user: user.into(),
    }))
}

async fn me(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AuthError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&pool)
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "User not found"))?;

    Ok(Json(UserPublic::from(user)))
}

/// Update user profile (username, bio, avatar_url)
async fn update_profile(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<UserPublic>, (StatusCode, Json<ErrorResponse>)> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&pool)
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "User not found"))?;

    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or(&user.username);
    let bio = body.get("bio").and_then(|v| v.as_str()).unwrap_or(user.bio.as_deref().unwrap_or(""));
    let avatar_url = body.get("avatar_url").and_then(|v| v.as_str()).or(user.avatar_url.as_deref());

    let _ = sqlx::query("UPDATE users SET username = $1, bio = $2, avatar_url = $3 WHERE id = $4")
        .bind(username)
        .bind(bio)
        .bind(avatar_url)
        .bind(auth.0)
        .execute(&pool)
        .await;

    let updated = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_one(&pool)
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(UserPublic::from(updated)))
}

/// Upload avatar image
async fn upload_avatar(
    State(pool): State<PgPool>,
    auth: AuthUser,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let mut image_data: Option<Vec<u8>> = None;
    let mut ext = "jpg".to_string();

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("avatar") {
            if let Some(ct) = field.content_type() {
                if ct.contains("png") { ext = "png".into(); }
                else if ct.contains("gif") { ext = "gif".into(); }
                else if ct.contains("webp") { ext = "webp".into(); }
            }
            if let Ok(data) = field.bytes().await {
                image_data = Some(data.to_vec());
            }
        }
    }

    let data = image_data.ok_or_else(|| err(StatusCode::BAD_REQUEST, "No avatar image provided"))?;

    let file_id = uuid::Uuid::new_v4();
    let path = format!("uploads/{file_id}.{ext}");
    tokio::fs::write(&path, &data).await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Save failed: {e}")))?;

    let url = format!("/{path}");
    let _ = sqlx::query("UPDATE users SET avatar_url = $1 WHERE id = $2")
        .bind(&url)
        .bind(auth.0)
        .execute(&pool)
        .await;

    Ok(Json(serde_json::json!({ "avatar_url": url })))
}

pub fn router() -> Router<PgPool> {
    use axum::routing::put;
    use axum::extract::DefaultBodyLimit;
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
        .route("/auth/profile", put(update_profile))
        .route("/auth/avatar", post(upload_avatar))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB for avatars
}
