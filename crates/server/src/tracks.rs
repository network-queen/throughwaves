use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::models::*;

// ── Upload track (multipart) ──

async fn create_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, StatusCode> {
    let mut title = String::new();
    let mut description = String::new();
    let mut genre = String::new();
    let mut bpm: Option<i32> = None;
    let mut key = String::new();
    let mut audio_filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "title" => {
                title = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            "description" => {
                description = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            "genre" => {
                genre = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            "bpm" => {
                let s = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                bpm = s.parse().ok();
            }
            "key" => {
                key = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            "audio" => {
                let original = field
                    .file_name()
                    .unwrap_or("upload.wav")
                    .to_string();
                let ext = original
                    .rsplit('.')
                    .next()
                    .unwrap_or("wav");
                let filename = format!("{}.{}", Uuid::new_v4(), ext);
                let path = format!("./uploads/{filename}");

                // Ensure uploads directory exists
                tokio::fs::create_dir_all("./uploads")
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                tokio::fs::write(&path, &bytes)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

                audio_filename = Some(filename);
            }
            _ => {}
        }
    }

    let audio_filename = audio_filename.ok_or(StatusCode::BAD_REQUEST)?;
    let audio_url = format!("/uploads/{audio_filename}");

    if title.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let track = sqlx::query_as::<_, Track>(
        r#"INSERT INTO tracks (user_id, title, description, audio_url, genre, bpm, key)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING *"#,
    )
    .bind(auth.0)
    .bind(&title)
    .bind(&description)
    .bind(&audio_url)
    .bind(&genre)
    .bind(bpm)
    .bind(&key)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("create track error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(track)))
}

// ── List tracks (paginated, filterable) ──

async fn list_tracks(
    State(pool): State<PgPool>,
    Query(q): Query<TrackQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let total: i64 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*)::bigint FROM tracks
           WHERE is_public = true
             AND ($1::varchar IS NULL OR genre = $1)
             AND ($2::uuid IS NULL OR user_id = $2)"#,
    )
    .bind(q.genre.as_deref())
    .bind(q.user_id)
    .fetch_one(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let tracks = sqlx::query_as::<_, Track>(
        r#"SELECT * FROM tracks
           WHERE is_public = true
             AND ($1::varchar IS NULL OR genre = $1)
             AND ($2::uuid IS NULL OR user_id = $2)
           ORDER BY created_at DESC
           LIMIT $3 OFFSET $4"#,
    )
    .bind(q.genre.as_deref())
    .bind(q.user_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(Paginated {
        data: tracks,
        page,
        per_page,
        total,
    }))
}

// ── Get track detail ──

async fn get_track(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let track = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let user = sqlx::query_as::<_, crate::models::User>("SELECT * FROM users WHERE id = $1")
        .bind(track.user_id)
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let raw_comments = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE track_id = $1 ORDER BY timestamp_seconds ASC NULLS LAST, created_at ASC",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut comments = Vec::with_capacity(raw_comments.len());
    for c in raw_comments {
        let cu = sqlx::query_as::<_, crate::models::User>("SELECT * FROM users WHERE id = $1")
            .bind(c.user_id)
            .fetch_one(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        comments.push(CommentWithUser {
            id: c.id,
            user: cu.into(),
            text: c.text,
            timestamp_seconds: c.timestamp_seconds,
            created_at: c.created_at,
        });
    }

    Ok(Json(TrackDetail {
        track,
        user: user.into(),
        comments,
    }))
}

// ── Toggle like ──

async fn like_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    // Check if already liked
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM track_likes WHERE user_id = $1 AND track_id = $2",
    )
    .bind(auth.0)
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if existing.is_some() {
        // Unlike
        sqlx::query("DELETE FROM track_likes WHERE user_id = $1 AND track_id = $2")
            .bind(auth.0)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        sqlx::query("UPDATE tracks SET likes = likes - 1 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(serde_json::json!({"liked": false})))
    } else {
        // Like
        sqlx::query("INSERT INTO track_likes (user_id, track_id) VALUES ($1, $2)")
            .bind(auth.0)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        sqlx::query("UPDATE tracks SET likes = likes + 1 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(serde_json::json!({"liked": true})))
    }
}

// ── Increment play count ──

async fn play_track(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    sqlx::query("UPDATE tracks SET plays = plays + 1 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Delete own track ──

async fn delete_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let result = sqlx::query("DELETE FROM tracks WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.0)
        .execute(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Post comment ──

async fn post_comment(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CommentRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let comment = sqlx::query_as::<_, Comment>(
        r#"INSERT INTO comments (user_id, track_id, text, timestamp_seconds)
           VALUES ($1, $2, $3, $4) RETURNING *"#,
    )
    .bind(auth.0)
    .bind(id)
    .bind(&body.text)
    .bind(body.timestamp_seconds)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("comment error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(comment)))
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/tracks", post(create_track).get(list_tracks))
        .route("/tracks/{id}", get(get_track).delete(delete_track))
        .route("/tracks/{id}/like", post(like_track))
        .route("/tracks/{id}/play", post(play_track))
        .route("/tracks/{id}/comments", post(post_comment))
}
