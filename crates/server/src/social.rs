use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::models::*;

/// Helper type for JSON error responses.
type SocialError = (StatusCode, Json<ErrorResponse>);

fn social_err(status: StatusCode, msg: &str) -> SocialError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

// ── Toggle follow ──

async fn toggle_follow(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, SocialError> {
    if auth.0 == user_id {
        return Err(social_err(StatusCode::BAD_REQUEST, "Cannot follow yourself"));
    }

    // Check target user exists
    let _target = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| social_err(StatusCode::NOT_FOUND, "User not found"))?;

    // Check if already following
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT follower_id FROM follows WHERE follower_id = $1 AND following_id = $2",
    )
    .bind(auth.0)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if existing.is_some() {
        // Unfollow
        sqlx::query("DELETE FROM follows WHERE follower_id = $1 AND following_id = $2")
            .bind(auth.0)
            .bind(user_id)
            .execute(&pool)
            .await
            .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        Ok(Json(serde_json::json!({"following": false})))
    } else {
        // Follow
        sqlx::query("INSERT INTO follows (follower_id, following_id) VALUES ($1, $2)")
            .bind(auth.0)
            .bind(user_id)
            .execute(&pool)
            .await
            .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        Ok(Json(serde_json::json!({"following": true})))
    }
}

// ── Get followers ──

async fn get_followers(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, SocialError> {
    let followers = sqlx::query_as::<_, User>(
        r#"SELECT u.* FROM users u
           JOIN follows f ON f.follower_id = u.id
           WHERE f.following_id = $1
           ORDER BY f.created_at DESC"#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let public: Vec<UserPublic> = followers.into_iter().map(|u| u.into()).collect();
    Ok(Json(public))
}

// ── Get following ──

async fn get_following(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, SocialError> {
    let following = sqlx::query_as::<_, User>(
        r#"SELECT u.* FROM users u
           JOIN follows f ON f.following_id = u.id
           WHERE f.follower_id = $1
           ORDER BY f.created_at DESC"#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let public: Vec<UserPublic> = following.into_iter().map(|u| u.into()).collect();
    Ok(Json(public))
}

// ── Get user profile ──

async fn get_user_profile(
    State(pool): State<PgPool>,
    auth: OptionalAuthUser,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, SocialError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| social_err(StatusCode::NOT_FOUND, "User not found"))?;

    let follower_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM follows WHERE following_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let following_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM follows WHERE follower_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let track_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM tracks WHERE user_id = $1 AND is_public = true",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let is_following = if let Some(auth_user_id) = auth.0 {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT follower_id FROM follows WHERE follower_id = $1 AND following_id = $2",
        )
        .bind(auth_user_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;
        existing.is_some()
    } else {
        false
    };

    Ok(Json(UserProfile {
        id: user.id,
        username: user.username,
        avatar_url: user.avatar_url,
        bio: user.bio,
        created_at: user.created_at,
        follower_count,
        following_count,
        track_count,
        is_following,
    }))
}

// ── Get user's tracks ──

async fn get_user_tracks(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, SocialError> {
    let tracks = sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks WHERE user_id = $1 AND is_public = true ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(tracks))
}

// ── Get user's reposts ──

async fn get_user_reposts(
    State(pool): State<PgPool>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, SocialError> {
    let tracks = sqlx::query_as::<_, Track>(
        r#"SELECT t.* FROM tracks t
           JOIN reposts r ON r.track_id = t.id
           WHERE r.user_id = $1 AND t.is_public = true
           ORDER BY r.created_at DESC"#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(tracks))
}

// ── Personalized feed ──

async fn get_feed(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Query(q): Query<TrackQuery>,
) -> Result<impl IntoResponse, SocialError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let total: i64 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*)::bigint FROM tracks t
           WHERE t.is_public = true
             AND t.user_id IN (SELECT following_id FROM follows WHERE follower_id = $1)"#,
    )
    .bind(auth.0)
    .fetch_one(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let tracks = sqlx::query_as::<_, Track>(
        r#"SELECT t.* FROM tracks t
           WHERE t.is_public = true
             AND t.user_id IN (SELECT following_id FROM follows WHERE follower_id = $1)
           ORDER BY t.created_at DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(auth.0)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|e| social_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(Paginated {
        data: tracks,
        page,
        per_page,
        total,
    }))
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/users/{id}", get(get_user_profile))
        .route("/users/{id}/follow", post(toggle_follow))
        .route("/users/{id}/followers", get(get_followers))
        .route("/users/{id}/following", get(get_following))
        .route("/users/{id}/tracks", get(get_user_tracks))
        .route("/users/{id}/reposts", get(get_user_reposts))
        .route("/feed", get(get_feed))
}
