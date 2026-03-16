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

use crate::models::{AuthResponse, LoginRequest, RegisterRequest, User, UserPublic};

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
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Uuid>()
            .copied()
            .map(AuthUser)
            .ok_or(StatusCode::UNAUTHORIZED)
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

fn make_token(user_id: Uuid) -> Result<String, StatusCode> {
    let exp = (Utc::now() + chrono::Duration::days(30)).timestamp() as usize;
    let claims = Claims { sub: user_id, exp };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// ── Handlers ──

async fn register(
    State(pool): State<PgPool>,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let hash = bcrypt::hash(&body.password, bcrypt::DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
        StatusCode::CONFLICT
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
) -> Result<impl IntoResponse, StatusCode> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&body.email)
        .fetch_optional(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let valid =
        bcrypt::verify(&body.password, &user.password_hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = make_token(user.id)?;

    Ok(Json(AuthResponse {
        token,
        user: user.into(),
    }))
}

async fn me(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> Result<impl IntoResponse, StatusCode> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(auth.0)
        .fetch_optional(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(UserPublic::from(user)))
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
}
