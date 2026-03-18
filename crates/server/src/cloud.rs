use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sha2::{Sha256, Digest};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::models::*;

/// Compute SHA-256 hash of audio data for delta detection
fn content_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Upload a cloud project: mixdown + individual stems.
/// If a project with the same title exists for this user, creates a new version
/// using delta storage (only changed stems are stored).
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
    let mut stems: Vec<(String, Vec<u8>)> = Vec::new();

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

    // Check if a project with the same title already exists for this user
    let existing: Option<CloudProject> = sqlx::query_as(
        "SELECT * FROM cloud_projects WHERE user_id = $1 AND title = $2"
    )
    .bind(user_id)
    .bind(&title)
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?;

    // Save mixdown file
    let mixdown_id = Uuid::new_v4();
    let mixdown_path = format!("uploads/{mixdown_id}.wav");
    tokio::fs::write(&mixdown_path, &mixdown_bytes).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Failed to save: {e}") })))?;
    let mixdown_url = format!("/{mixdown_path}");

    let waveform = crate::tracks::generate_waveform_public(&mixdown_bytes);
    let duration = crate::tracks::estimate_wav_duration(&mixdown_bytes);

    if let Some(existing_project) = existing {
        // ── NEW VERSION of existing project (delta storage) ──
        let project_id = existing_project.id;

        // Get previous version's stems for delta comparison
        let prev_stems: Vec<CloudProjectStem> = sqlx::query_as(
            "SELECT * FROM cloud_project_stems WHERE cloud_project_id = $1 ORDER BY track_index"
        )
        .bind(project_id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        // Build hash map of previous stems: name → (stem_id, content_hash)
        let prev_hash_map: std::collections::HashMap<String, (Uuid, String)> = prev_stems.iter()
            .filter_map(|s| {
                s.content_hash.as_ref().map(|h| (s.name.clone(), (s.id, h.clone())))
            })
            .collect();

        // Process new stems with delta detection
        let mut stem_refs = Vec::new();
        let mut new_count = 0;
        let mut reused_count = 0;

        for (idx, (stem_name, stem_data)) in stems.iter().enumerate() {
            let hash = content_hash(stem_data);

            // Check if this stem is unchanged from previous version
            if let Some((prev_id, prev_hash)) = prev_hash_map.get(stem_name) {
                if &hash == prev_hash {
                    // Reuse existing stem — no new file needed (delta!)
                    stem_refs.push(serde_json::json!({
                        "stem_id": prev_id,
                        "name": stem_name,
                        "track_index": idx,
                        "is_new": false,
                    }));
                    reused_count += 1;
                    continue;
                }
            }

            // New or changed stem — save file
            let stem_file_id = Uuid::new_v4();
            let stem_path = format!("uploads/{stem_file_id}.wav");
            if let Err(e) = tokio::fs::write(&stem_path, stem_data).await {
                eprintln!("Failed to save stem {stem_name}: {e}");
                continue;
            }
            let stem_url = format!("/{stem_path}");

            let stem_id: Uuid = sqlx::query_scalar(
                "INSERT INTO cloud_project_stems (cloud_project_id, name, audio_url, track_index, content_hash)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id"
            )
            .bind(project_id)
            .bind(stem_name)
            .bind(&stem_url)
            .bind(idx as i32)
            .bind(&hash)
            .fetch_one(&pool)
            .await
            .unwrap_or(Uuid::new_v4());

            stem_refs.push(serde_json::json!({
                "stem_id": stem_id,
                "name": stem_name,
                "track_index": idx,
                "is_new": true,
            }));
            new_count += 1;
        }

        // Get next version number
        let max_ver: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(version_number) FROM cloud_project_versions WHERE cloud_project_id = $1"
        )
        .bind(project_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
        let next_ver = max_ver.unwrap_or(0) + 1;

        // If this is the first version (upgrading from v0), create v1 for the original
        if next_ver == 1 {
            let orig_stem_refs: Vec<serde_json::Value> = prev_stems.iter().map(|s| {
                serde_json::json!({
                    "stem_id": s.id,
                    "name": s.name,
                    "track_index": s.track_index,
                    "is_new": true,
                })
            }).collect();
            let _ = sqlx::query(
                "INSERT INTO cloud_project_versions (cloud_project_id, version_number, message, mixdown_url, waveform_data, duration_seconds, stem_refs)
                 VALUES ($1, 1, 'Initial upload', $2, $3, $4, $5)"
            )
            .bind(project_id)
            .bind(&existing_project.mixdown_url)
            .bind(&existing_project.waveform_data)
            .bind(existing_project.duration_seconds)
            .bind(serde_json::json!(orig_stem_refs))
            .execute(&pool)
            .await;
        }

        let ver_num = if next_ver == 1 { 2 } else { next_ver };

        // Create version record
        let _ = sqlx::query(
            "INSERT INTO cloud_project_versions (cloud_project_id, version_number, message, mixdown_url, waveform_data, duration_seconds, stem_refs)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(project_id)
        .bind(ver_num)
        .bind(format!("Version {ver_num}"))
        .bind(&mixdown_url)
        .bind(&waveform)
        .bind(duration)
        .bind(serde_json::json!(stem_refs))
        .execute(&pool)
        .await;

        // Update the project's current mixdown
        let _ = sqlx::query(
            "UPDATE cloud_projects SET mixdown_url = $1, waveform_data = $2, duration_seconds = $3, bpm = $4 WHERE id = $5"
        )
        .bind(&mixdown_url)
        .bind(&waveform)
        .bind(duration)
        .bind(bpm)
        .bind(project_id)
        .execute(&pool)
        .await;

        println!("[CLOUD] Project v{ver_num}: {title} ({new_count} new, {reused_count} reused stems)");

        Ok(Json(serde_json::json!({
            "id": project_id,
            "title": title,
            "version": ver_num,
            "new_stems": new_count,
            "reused_stems": reused_count,
        })))
    } else {
        // ── FIRST UPLOAD — create new project ──
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?;

        // Save stems with content hashes
        let mut stem_refs = Vec::new();
        for (idx, (stem_name, stem_data)) in stems.iter().enumerate() {
            let hash = content_hash(stem_data);
            let stem_file_id = Uuid::new_v4();
            let stem_path = format!("uploads/{stem_file_id}.wav");
            if let Err(e) = tokio::fs::write(&stem_path, stem_data).await {
                eprintln!("Failed to save stem {stem_name}: {e}");
                continue;
            }
            let stem_url = format!("/{stem_path}");
            let stem_id: Uuid = sqlx::query_scalar(
                "INSERT INTO cloud_project_stems (cloud_project_id, name, audio_url, track_index, content_hash)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id"
            )
            .bind(project_id)
            .bind(stem_name)
            .bind(&stem_url)
            .bind(idx as i32)
            .bind(&hash)
            .fetch_one(&pool)
            .await
            .unwrap_or(Uuid::new_v4());

            stem_refs.push(serde_json::json!({
                "stem_id": stem_id,
                "name": stem_name,
                "track_index": idx,
                "is_new": true,
            }));
        }

        // Create initial version record
        let _ = sqlx::query(
            "INSERT INTO cloud_project_versions (cloud_project_id, version_number, message, mixdown_url, waveform_data, duration_seconds, stem_refs)
             VALUES ($1, 1, 'Initial upload', $2, $3, $4, $5)"
        )
        .bind(project_id)
        .bind(&mixdown_url)
        .bind(&waveform)
        .bind(duration)
        .bind(serde_json::json!(stem_refs))
        .execute(&pool)
        .await;

        println!("[CLOUD] New project: {title} ({} stems)", stems.len());

        Ok(Json(serde_json::json!({
            "id": project_id,
            "title": title,
            "version": 1,
            "new_stems": stems.len(),
            "reused_stems": 0,
        })))
    }
}

/// List cloud projects
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

/// Get a cloud project with stems and version history
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

    let _ = sqlx::query("UPDATE cloud_projects SET plays = plays + 1 WHERE id = $1")
        .bind(project_id).execute(&pool).await;

    let is_owner = project.user_id == auth.0;

    let stems = if is_owner {
        sqlx::query_as::<_, CloudProjectStem>(
            "SELECT * FROM cloud_project_stems WHERE cloud_project_id = $1 ORDER BY track_index"
        )
        .bind(project_id).fetch_all(&pool).await.unwrap_or_default()
    } else { Vec::new() };

    let versions = if is_owner {
        sqlx::query_as::<_, CloudProjectVersion>(
            "SELECT * FROM cloud_project_versions WHERE cloud_project_id = $1 ORDER BY version_number DESC"
        )
        .bind(project_id).fetch_all(&pool).await.unwrap_or_default()
    } else { Vec::new() };

    let username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(project.user_id).fetch_optional(&pool).await.ok().flatten();

    Ok(Json(serde_json::json!({
        "project": project,
        "stems": stems,
        "versions": versions,
        "is_owner": is_owner,
        "username": username.unwrap_or_else(|| "Unknown".into()),
    })))
}

/// Download stems (owner only)
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
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Owner only".into() })));
    }

    let stems = sqlx::query_as::<_, CloudProjectStem>(
        "SELECT * FROM cloud_project_stems WHERE cloud_project_id = $1 ORDER BY track_index"
    )
    .bind(project_id).fetch_all(&pool).await.unwrap_or_default();

    Ok(Json(serde_json::json!({
        "project": { "id": project.id, "title": project.title, "mixdown_url": project.mixdown_url },
        "stems": stems,
    })))
}

/// Delete a cloud project (owner only)
async fn delete_cloud_project(
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
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Not found".into() })))?;

    if project.user_id != auth.0 {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Owner only".into() })));
    }

    // Delete cascades to stems and versions via FK
    let _ = sqlx::query("DELETE FROM cloud_projects WHERE id = $1")
        .bind(project_id).execute(&pool).await;

    println!("[CLOUD] Deleted project: {}", project.title);
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

/// Update cloud project details (owner only)
async fn update_cloud_project(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let project = sqlx::query_as::<_, CloudProject>(
        "SELECT * FROM cloud_projects WHERE id = $1"
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Not found".into() })))?;

    if project.user_id != auth.0 {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Owner only".into() })));
    }

    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or(&project.title);
    let description = body.get("description").and_then(|v| v.as_str())
        .unwrap_or(project.description.as_deref().unwrap_or(""));
    let genre = body.get("genre").and_then(|v| v.as_str())
        .unwrap_or(project.genre.as_deref().unwrap_or(""));
    let bpm = body.get("bpm").and_then(|v| v.as_i64()).map(|v| v as i32)
        .or(project.bpm);
    let is_public = body.get("is_public").and_then(|v| v.as_bool())
        .unwrap_or(project.is_public.unwrap_or(true));

    let _ = sqlx::query(
        "UPDATE cloud_projects SET title = $1, description = $2, genre = $3, bpm = $4, is_public = $5 WHERE id = $6"
    )
    .bind(title)
    .bind(description)
    .bind(genre)
    .bind(bpm)
    .bind(is_public)
    .bind(project_id)
    .execute(&pool)
    .await;

    println!("[CLOUD] Updated project: {title}");
    Ok(Json(serde_json::json!({
        "id": project_id,
        "title": title,
        "description": description,
        "genre": genre,
        "bpm": bpm,
        "is_public": is_public,
    })))
}

pub fn router() -> Router<PgPool> {
    use axum::routing::{delete, put};
    Router::new()
        .route("/cloud", post(upload_cloud_project).get(list_cloud_projects))
        .route("/cloud/{id}", get(get_cloud_project).delete(delete_cloud_project).put(update_cloud_project))
        .route("/cloud/{id}/download", post(download_cloud_project))
}
