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

// ── WAV Waveform Generation ──

/// Parse a WAV file and generate normalized waveform peaks (0.0 to 1.0).
/// Returns (peaks, duration_seconds). For non-WAV files, returns None.
fn generate_waveform_from_wav(data: &[u8], num_bins: usize) -> Option<(Vec<f64>, f64)> {
    // Minimal WAV parser: check RIFF header
    if data.len() < 44 {
        return None;
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }

    // Find fmt chunk
    let mut pos = 12;
    let mut sample_rate: u32 = 44100;
    let mut bits_per_sample: u16 = 16;
    let mut num_channels: u16 = 2;
    let mut data_start = 0usize;
    let mut data_size = 0usize;

    while pos + 8 <= data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) as usize;

        if chunk_id == b"fmt " && pos + 8 + chunk_size <= data.len() {
            // audio_format = u16 at pos+8 (1 = PCM)
            num_channels = u16::from_le_bytes([data[pos + 10], data[pos + 11]]);
            sample_rate = u32::from_le_bytes([data[pos + 12], data[pos + 13], data[pos + 14], data[pos + 15]]);
            bits_per_sample = u16::from_le_bytes([data[pos + 22], data[pos + 23]]);
        } else if chunk_id == b"data" {
            data_start = pos + 8;
            data_size = chunk_size.min(data.len() - data_start);
            break;
        }

        pos += 8 + chunk_size;
        // Align to even byte
        if pos % 2 != 0 {
            pos += 1;
        }
    }

    if data_start == 0 || data_size == 0 || bits_per_sample == 0 || num_channels == 0 {
        return None;
    }

    let bytes_per_sample = (bits_per_sample / 8) as usize;
    let frame_size = bytes_per_sample * num_channels as usize;
    if frame_size == 0 {
        return None;
    }
    let total_frames = data_size / frame_size;
    let duration = total_frames as f64 / sample_rate as f64;

    if total_frames == 0 {
        return None;
    }

    let frames_per_bin = (total_frames + num_bins - 1) / num_bins;
    let mut peaks = Vec::with_capacity(num_bins);

    for bin in 0..num_bins {
        let start_frame = bin * frames_per_bin;
        let end_frame = ((bin + 1) * frames_per_bin).min(total_frames);
        let mut max_val: f64 = 0.0;

        for frame in start_frame..end_frame {
            let frame_offset = data_start + frame * frame_size;
            // Read first channel only
            if frame_offset + bytes_per_sample <= data.len() {
                let sample_abs = match bits_per_sample {
                    8 => {
                        // 8-bit WAV is unsigned
                        let s = data[frame_offset] as f64 - 128.0;
                        s.abs() / 128.0
                    }
                    16 => {
                        let s = i16::from_le_bytes([data[frame_offset], data[frame_offset + 1]]);
                        (s as f64).abs() / 32768.0
                    }
                    24 => {
                        let s = i32::from_le_bytes([0, data[frame_offset], data[frame_offset + 1], data[frame_offset + 2]]);
                        (s as f64).abs() / 8388608.0
                    }
                    32 => {
                        let s = i32::from_le_bytes([data[frame_offset], data[frame_offset + 1], data[frame_offset + 2], data[frame_offset + 3]]);
                        (s as f64).abs() / 2147483648.0
                    }
                    _ => 0.0,
                };
                if sample_abs > max_val {
                    max_val = sample_abs;
                }
            }
        }
        peaks.push(max_val);
    }

    // Normalize peaks so max = 1.0
    let global_max = peaks.iter().copied().fold(0.0f64, f64::max);
    if global_max > 0.0 {
        for p in &mut peaks {
            *p /= global_max;
        }
    }

    Some((peaks, duration))
}

/// Helper type for JSON error responses.
type TrackError = (StatusCode, Json<ErrorResponse>);

fn track_err(status: StatusCode, msg: &str) -> TrackError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

// ── Upload track (multipart) ──

async fn create_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, TrackError> {
    println!("[TRACKS] Upload request from user_id={}", auth.0);

    let mut title = String::new();
    let mut description = String::new();
    let mut genre = String::new();
    let mut bpm: Option<i32> = None;
    let mut key = String::new();
    let mut audio_filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| {
            eprintln!("[TRACKS] Multipart parse error: {e}");
            track_err(StatusCode::BAD_REQUEST, &format!("Invalid multipart data: {e}"))
        })?
    {
        let name = field.name().unwrap_or("").to_string();
        println!("[TRACKS] Processing field: {name:?}");
        match name.as_str() {
            "title" => {
                title = field.text().await.map_err(|e| {
                    track_err(StatusCode::BAD_REQUEST, &format!("Failed to read title: {e}"))
                })?;
            }
            "description" => {
                description = field.text().await.map_err(|e| {
                    track_err(StatusCode::BAD_REQUEST, &format!("Failed to read description: {e}"))
                })?;
            }
            "genre" => {
                genre = field.text().await.map_err(|e| {
                    track_err(StatusCode::BAD_REQUEST, &format!("Failed to read genre: {e}"))
                })?;
            }
            "bpm" => {
                let s = field.text().await.map_err(|e| {
                    track_err(StatusCode::BAD_REQUEST, &format!("Failed to read bpm: {e}"))
                })?;
                bpm = s.parse().ok();
            }
            "key" => {
                key = field.text().await.map_err(|e| {
                    track_err(StatusCode::BAD_REQUEST, &format!("Failed to read key: {e}"))
                })?;
            }
            "audio" | "file" => {
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

                println!("[TRACKS] Receiving audio file: {original} -> {filename}");

                // Ensure uploads directory exists
                tokio::fs::create_dir_all("./uploads")
                    .await
                    .map_err(|_| track_err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to create uploads directory"))?;

                let bytes = field.bytes().await.map_err(|e| {
                    eprintln!("[TRACKS] Failed to read file bytes: {e}");
                    track_err(StatusCode::BAD_REQUEST, &format!("Failed to read audio data: {e}"))
                })?;

                println!("[TRACKS] Received {} bytes, writing to {path}", bytes.len());

                tokio::fs::write(&path, &bytes)
                    .await
                    .map_err(|e| {
                        eprintln!("[TRACKS] Failed to write file: {e}");
                        track_err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save audio file")
                    })?;

                audio_filename = Some(filename);
            }
            other => {
                println!("[TRACKS] Ignoring unknown field: {other:?}");
            }
        }
    }

    let audio_filename = match audio_filename {
        Some(f) => f,
        None => {
            eprintln!("[TRACKS] Upload error: no audio file received. Title={title:?}");
            return Err(track_err(StatusCode::BAD_REQUEST, "No audio file received. Send file in a field named 'audio'."));
        }
    };
    let audio_url = format!("/uploads/{audio_filename}");

    if title.is_empty() {
        eprintln!("[TRACKS] Upload error: empty title");
        return Err(track_err(StatusCode::BAD_REQUEST, "Title is required"));
    }

    // Generate waveform data from the uploaded file
    let path = format!("./uploads/{audio_filename}");
    let file_bytes = tokio::fs::read(&path).await.unwrap_or_default();
    let (waveform_json, duration_seconds) = match generate_waveform_from_wav(&file_bytes, 200) {
        Some((peaks, dur)) => {
            println!("[TRACKS] Generated waveform: {} peaks, {:.1}s duration", peaks.len(), dur);
            (Some(serde_json::json!(peaks)), Some(dur))
        }
        None => {
            println!("[TRACKS] Non-WAV file or parse failed, skipping waveform generation");
            (None, None)
        }
    };

    println!("[TRACKS] Inserting track: title={title:?}, genre={genre:?}, bpm={bpm:?}");

    let track = sqlx::query_as::<_, Track>(
        r#"INSERT INTO tracks (user_id, title, description, audio_url, genre, bpm, key, waveform_data, duration_seconds)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           RETURNING *"#,
    )
    .bind(auth.0)
    .bind(&title)
    .bind(&description)
    .bind(&audio_url)
    .bind(&genre)
    .bind(bpm)
    .bind(&key)
    .bind(&waveform_json)
    .bind(duration_seconds)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("[TRACKS] create track DB error: {e}");
        track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}"))
    })?;

    println!("[TRACKS] Track created: id={}", track.id);

    Ok((StatusCode::CREATED, Json(track)))
}

// ── List tracks (paginated, filterable) ──

async fn list_tracks(
    State(pool): State<PgPool>,
    Query(q): Query<TrackQuery>,
) -> Result<impl IntoResponse, TrackError> {
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
    .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

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
    .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

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
) -> Result<impl IntoResponse, TrackError> {
    let track = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| track_err(StatusCode::NOT_FOUND, "Track not found"))?;

    let user = sqlx::query_as::<_, crate::models::User>("SELECT * FROM users WHERE id = $1")
        .bind(track.user_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let raw_comments = sqlx::query_as::<_, Comment>(
        "SELECT * FROM comments WHERE track_id = $1 ORDER BY timestamp_seconds ASC NULLS LAST, created_at ASC",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let mut comments = Vec::with_capacity(raw_comments.len());
    for c in raw_comments {
        let cu = sqlx::query_as::<_, crate::models::User>("SELECT * FROM users WHERE id = $1")
            .bind(c.user_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;
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
) -> Result<impl IntoResponse, TrackError> {
    // Check if already liked
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM track_likes WHERE user_id = $1 AND track_id = $2",
    )
    .bind(auth.0)
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if existing.is_some() {
        // Unlike
        sqlx::query("DELETE FROM track_likes WHERE user_id = $1 AND track_id = $2")
            .bind(auth.0)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        sqlx::query("UPDATE tracks SET likes = likes - 1 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        Ok(Json(serde_json::json!({"liked": false})))
    } else {
        // Like
        sqlx::query("INSERT INTO track_likes (user_id, track_id) VALUES ($1, $2)")
            .bind(auth.0)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        sqlx::query("UPDATE tracks SET likes = likes + 1 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        Ok(Json(serde_json::json!({"liked": true})))
    }
}

// ── Increment play count ──

async fn play_track(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, TrackError> {
    sqlx::query("UPDATE tracks SET plays = plays + 1 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Delete own track ──

async fn delete_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, TrackError> {
    let result = sqlx::query("DELETE FROM tracks WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(auth.0)
        .execute(&pool)
        .await
        .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if result.rows_affected() == 0 {
        return Err(track_err(StatusCode::NOT_FOUND, "Track not found or not owned by you"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Post comment ──

async fn post_comment(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CommentRequest>,
) -> Result<impl IntoResponse, TrackError> {
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
        track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}"))
    })?;

    Ok((StatusCode::CREATED, Json(comment)))
}

// ── Toggle repost ──

async fn repost_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, TrackError> {
    // Check if already reposted
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM reposts WHERE user_id = $1 AND track_id = $2",
    )
    .bind(auth.0)
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if existing.is_some() {
        // Un-repost
        sqlx::query("DELETE FROM reposts WHERE user_id = $1 AND track_id = $2")
            .bind(auth.0)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM reposts WHERE track_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        Ok(Json(serde_json::json!({"reposted": false, "repost_count": count})))
    } else {
        // Repost
        sqlx::query("INSERT INTO reposts (user_id, track_id) VALUES ($1, $2)")
            .bind(auth.0)
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM reposts WHERE track_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .map_err(|e| track_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        Ok(Json(serde_json::json!({"reposted": true, "repost_count": count})))
    }
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/tracks", post(create_track).get(list_tracks))
        .route("/tracks/{id}", get(get_track).delete(delete_track))
        .route("/tracks/{id}/like", post(like_track))
        .route("/tracks/{id}/play", post(play_track))
        .route("/tracks/{id}/comments", post(post_comment))
        .route("/tracks/{id}/repost", post(repost_track))
}

/// Public wrapper for waveform generation (used by cloud.rs)
pub fn generate_waveform_public(data: &[u8]) -> Option<serde_json::Value> {
    generate_waveform_from_wav(data, 200).map(|(peaks, _)| serde_json::json!(peaks))
}

/// Estimate WAV duration from raw bytes
pub fn estimate_wav_duration(data: &[u8]) -> Option<f64> {
    generate_waveform_from_wav(data, 200).map(|(_, dur)| dur)
}
