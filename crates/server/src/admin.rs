use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::models::ErrorResponse;

type AdminError = (StatusCode, Json<ErrorResponse>);

fn admin_err(status: StatusCode, msg: &str) -> AdminError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

/// Check if the authenticated user is an admin
async fn require_admin(pool: &PgPool, user_id: Uuid) -> Result<(), AdminError> {
    let is_admin: bool = sqlx::query_scalar::<_, Option<bool>>(
        "SELECT is_admin FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
    .unwrap_or(false);

    if !is_admin {
        return Err(admin_err(StatusCode::FORBIDDEN, "Admin access required"));
    }
    Ok(())
}

// ── Stats ──

#[derive(Debug, Serialize)]
struct AdminStats {
    user_count: i64,
    track_count: i64,
    cloud_project_count: i64,
}

async fn get_stats(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM users")
        .fetch_one(&pool)
        .await
        .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let track_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM tracks")
        .fetch_one(&pool)
        .await
        .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let cloud_project_count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM cloud_projects")
        .fetch_one(&pool)
        .await
        .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(AdminStats {
        user_count,
        track_count,
        cloud_project_count,
    }))
}

// ── Users ──

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AdminUser {
    id: Uuid,
    username: String,
    email: String,
    is_admin: Option<bool>,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_users(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    let users = sqlx::query_as::<_, AdminUser>(
        "SELECT id, username, email, is_admin, created_at FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(users))
}

async fn delete_user(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    // Prevent deleting yourself
    if user_id == auth.0 {
        return Err(admin_err(StatusCode::BAD_REQUEST, "Cannot delete your own account"));
    }

    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(admin_err(StatusCode::NOT_FOUND, "User not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Tracks ──

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AdminTrack {
    id: Uuid,
    user_id: Uuid,
    title: String,
    genre: Option<String>,
    plays: Option<i64>,
    likes: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_tracks(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    let tracks = sqlx::query_as::<_, AdminTrack>(
        "SELECT id, user_id, title, genre, plays, likes, created_at FROM tracks ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(tracks))
}

async fn delete_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(track_id): Path<Uuid>,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    let result = sqlx::query("DELETE FROM tracks WHERE id = $1")
        .bind(track_id)
        .execute(&pool)
        .await
        .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(admin_err(StatusCode::NOT_FOUND, "Track not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Cloud Projects ──

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AdminCloudProject {
    id: Uuid,
    user_id: Uuid,
    title: String,
    genre: Option<String>,
    plays: Option<i64>,
    is_public: Option<bool>,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_cloud_projects(
    State(pool): State<PgPool>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    let projects = sqlx::query_as::<_, AdminCloudProject>(
        "SELECT id, user_id, title, genre, plays, is_public, created_at FROM cloud_projects ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(projects))
}

async fn delete_cloud_project(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, AdminError> {
    require_admin(&pool, auth.0).await?;

    let result = sqlx::query("DELETE FROM cloud_projects WHERE id = $1")
        .bind(project_id)
        .execute(&pool)
        .await
        .map_err(|e| admin_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(admin_err(StatusCode::NOT_FOUND, "Cloud project not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/admin/stats", get(get_stats))
        .route("/admin/users", get(list_users))
        .route("/admin/users/{id}", delete(delete_user))
        .route("/admin/tracks", get(list_tracks))
        .route("/admin/tracks/{id}", delete(delete_track))
        .route("/admin/cloud", get(list_cloud_projects))
        .route("/admin/cloud/{id}", delete(delete_cloud_project))
}
