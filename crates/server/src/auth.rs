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

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
}
