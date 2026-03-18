use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::models::*;

/// Upload a cloud project: mixdown + individual stems
async fn upload_cloud_project(
    State(pool): State<PgPool>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let user_id = auth.0;
    let mut title = String::new();
    let mut description = String::new();
    let mut genre = String::new();
    let mut bpm: Option<i32> = None;
    let mut mixdown_data: Option<Vec<u8>> = None;
    let mut stems: Vec<(String, Vec<u8>)> = Vec::new(); // (name, data)

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "title" => title = field.text().await.unwrap_or_default(),
            "description" => description = field.text().await.unwrap_or_default(),
            "genre" => genre = field.text().await.unwrap_or_default(),
            "bpm" => bpm = field.text().await.ok().and_then(|s| s.parse().ok()),
            "mixdown" => {
                if let Ok(data) = field.bytes().await {
                    mixdown_data = Some(data.to_vec());
                }
            }
            name if name.starts_with("stem_") => {
                let stem_name = name.strip_prefix("stem_").unwrap_or(name).to_string();
                if let Ok(data) = field.bytes().await {
                    stems.push((stem_name, data.to_vec()));
                }
            }
            _ => {}
        }
    }

    if title.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Title is required".into() })));
    }
    let mixdown_bytes = match mixdown_data {
        Some(d) => d,
        None => return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Mixdown audio is required".into() }))),
    };

    // Save mixdown file
    let mixdown_id = Uuid::new_v4();
    let mixdown_path = format!("uploads/{mixdown_id}.wav");
    tokio::fs::write(&mixdown_path, &mixdown_bytes).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Failed to save mixdown: {e}") })))?;

    let mixdown_url = format!("/{mixdown_path}");

    // Generate waveform from mixdown
    let waveform = crate::tracks::generate_waveform_public(&mixdown_bytes);
    let duration = crate::tracks::estimate_wav_duration(&mixdown_bytes);

    // Insert cloud project
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cloud_projects (user_id, title, description, mixdown_url, waveform_data, duration_seconds, genre, bpm)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id"
    )
    .bind(user_id)
    .bind(&title)
    .bind(&description)
    .bind(&mixdown_url)
    .bind(&waveform)
    .bind(duration)
    .bind(&genre)
    .bind(bpm)
    .fetch_one(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB error: {e}") })))?;

    // Save stems
    for (idx, (stem_name, stem_data)) in stems.iter().enumerate() {
        let stem_id = Uuid::new_v4();
        let stem_path = format!("uploads/{stem_id}.wav");
        if let Err(e) = tokio::fs::write(&stem_path, stem_data).await {
            eprintln!("Failed to save stem {stem_name}: {e}");
            continue;
        }
        let stem_url = format!("/{stem_path}");
        let _ = sqlx::query(
            "INSERT INTO cloud_project_stems (cloud_project_id, name, audio_url, track_index)
             VALUES ($1, $2, $3, $4)"
        )
        .bind(project_id)
        .bind(stem_name)
        .bind(&stem_url)
        .bind(idx as i32)
        .execute(&pool)
        .await;
    }

    println!("[CLOUD] Project uploaded: {title} ({} stems)", stems.len());

    Ok(Json(serde_json::json!({
        "id": project_id,
        "title": title,
        "stems": stems.len(),
    })))
}

/// List cloud projects (public ones, or all for the owner)
async fn list_cloud_projects(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let projects = sqlx::query_as::<_, CloudProject>(
        "SELECT * FROM cloud_projects WHERE is_public = true ORDER BY created_at DESC LIMIT 50"
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?;

    Ok(Json(serde_json::json!({ "data": projects })))
}

/// Get a cloud project with its stems (owner sees stems, others see only mixdown)
async fn get_cloud_project(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let project = sqlx::query_as::<_, CloudProject>(
        "SELECT * FROM cloud_projects WHERE id = $1"
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Project not found".into() })))?;

    // Increment plays
    let _ = sqlx::query("UPDATE cloud_projects SET plays = plays + 1 WHERE id = $1")
        .bind(project_id)
        .execute(&pool)
        .await;

    // Only owner can see stems
    let is_owner = project.user_id == auth.0;
    let stems = if is_owner {
        sqlx::query_as::<_, CloudProjectStem>(
            "SELECT * FROM cloud_project_stems WHERE cloud_project_id = $1 ORDER BY track_index"
        )
        .bind(project_id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Get username
    let username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(project.user_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();

    Ok(Json(serde_json::json!({
        "project": project,
        "stems": stems,
        "is_owner": is_owner,
        "username": username.unwrap_or_else(|| "Unknown".into()),
    })))
}

/// Download all stems of a cloud project (owner only) — returns JSON with stem URLs
async fn download_cloud_project(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let project = sqlx::query_as::<_, CloudProject>(
        "SELECT * FROM cloud_projects WHERE id = $1"
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Project not found".into() })))?;

    if project.user_id != auth.0 {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Only the project owner can download stems".into() })));
    }

    let stems = sqlx::query_as::<_, CloudProjectStem>(
        "SELECT * FROM cloud_project_stems WHERE cloud_project_id = $1 ORDER BY track_index"
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "project": {
            "id": project.id,
            "title": project.title,
            "mixdown_url": project.mixdown_url,
        },
        "stems": stems,
    })))
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/cloud", post(upload_cloud_project).get(list_cloud_projects))
        .route("/cloud/{id}", get(get_cloud_project))
        .route("/cloud/{id}/download", post(download_cloud_project))
}
