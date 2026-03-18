use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put, delete},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::models::*;

// ── Create Band ──

async fn create_band(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Band name is required".into() })));
    }

    let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let genre = body.get("genre").and_then(|v| v.as_str()).unwrap_or("");
    let website = body.get("website").and_then(|v| v.as_str()).unwrap_or("");
    let location = body.get("location").and_then(|v| v.as_str()).unwrap_or("");

    let band_id: Uuid = sqlx::query_scalar(
        "INSERT INTO bands (name, description, genre, website, location, created_by) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id"
    )
    .bind(name).bind(description).bind(genre).bind(website).bind(location).bind(auth.0)
    .fetch_one(&pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?;

    // Add creator as admin member
    let _ = sqlx::query("INSERT INTO band_members (band_id, user_id, role) VALUES ($1,$2,'admin')")
        .bind(band_id).bind(auth.0).execute(&pool).await;

    Ok(Json(serde_json::json!({ "id": band_id, "name": name })))
}

// ── List Bands ──

async fn list_bands(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let bands = sqlx::query_as::<_, Band>(
        "SELECT * FROM bands WHERE is_public = true ORDER BY created_at DESC LIMIT 50"
    ).fetch_all(&pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?;

    Ok(Json(serde_json::json!({ "data": bands })))
}

// ── Get Band with members and projects ──

async fn get_band(
    State(pool): State<PgPool>,
    Path(band_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let band = sqlx::query_as::<_, Band>("SELECT * FROM bands WHERE id = $1")
        .bind(band_id).fetch_optional(&pool).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Band not found".into() })))?;

    // Get members with usernames
    let members: Vec<serde_json::Value> = sqlx::query_as::<_, BandMember>(
        "SELECT * FROM band_members WHERE band_id = $1 ORDER BY joined_at"
    ).bind(band_id).fetch_all(&pool).await.unwrap_or_default()
    .into_iter().map(|m| serde_json::json!(m)).collect();

    // Get member usernames and avatars
    let mut members_with_info = Vec::new();
    for m in &members {
        let uid = m.get("user_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<Uuid>().ok());
        if let Some(uid) = uid {
            let user: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT username, avatar_url FROM users WHERE id = $1"
            ).bind(uid).fetch_optional(&pool).await.ok().flatten();
            let (username, avatar) = user.unwrap_or(("Unknown".into(), None));
            let mut info = m.clone();
            if let Some(obj) = info.as_object_mut() {
                obj.insert("username".into(), serde_json::json!(username));
                obj.insert("avatar_url".into(), serde_json::json!(avatar));
            }
            members_with_info.push(info);
        }
    }

    // Get band's cloud projects
    let projects = sqlx::query_as::<_, CloudProject>(
        "SELECT * FROM cloud_projects WHERE band_id = $1 ORDER BY created_at DESC"
    ).bind(band_id).fetch_all(&pool).await.unwrap_or_default();

    // Get published tracks by band members
    let member_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM band_members WHERE band_id = $1"
    ).bind(band_id).fetch_all(&pool).await.unwrap_or_default();

    let mut tracks = Vec::new();
    for uid in &member_ids {
        let user_tracks = sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE user_id = $1 AND is_public = true ORDER BY created_at DESC LIMIT 10"
        ).bind(uid).fetch_all(&pool).await.unwrap_or_default();
        tracks.extend(user_tracks);
    }

    Ok(Json(serde_json::json!({
        "band": band,
        "members": members_with_info,
        "projects": projects,
        "tracks": tracks,
    })))
}

// ── Update Band ──

async fn update_band(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(band_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Check if user is band admin
    let is_admin: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM band_members WHERE band_id = $1 AND user_id = $2 AND role = 'admin'"
    ).bind(band_id).bind(auth.0).fetch_one(&pool).await.unwrap_or(0) > 0;

    if !is_admin {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Only band admins can edit".into() })));
    }

    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let genre = body.get("genre").and_then(|v| v.as_str()).unwrap_or("");
    let website = body.get("website").and_then(|v| v.as_str()).unwrap_or("");
    let location = body.get("location").and_then(|v| v.as_str()).unwrap_or("");

    let _ = sqlx::query(
        "UPDATE bands SET name=$1, description=$2, genre=$3, website=$4, location=$5 WHERE id=$6"
    ).bind(name).bind(description).bind(genre).bind(website).bind(location).bind(band_id)
    .execute(&pool).await;

    Ok(Json(serde_json::json!({ "status": "updated" })))
}

// ── Upload Band Avatar/Banner ──

async fn upload_band_image(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(band_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let is_admin: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM band_members WHERE band_id = $1 AND user_id = $2 AND role = 'admin'"
    ).bind(band_id).bind(auth.0).fetch_one(&pool).await.unwrap_or(0) > 0;

    if !is_admin {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Only band admins can upload images".into() })));
    }

    let mut result = serde_json::json!({});
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();
        let ext = if field.content_type().map_or(false, |c| c.contains("png")) { "png" } else { "jpg" };
        if let Ok(data) = field.bytes().await {
            let file_id = Uuid::new_v4();
            let path = format!("uploads/{file_id}.{ext}");
            let _ = tokio::fs::write(&path, &data).await;
            let url = format!("/{path}");
            match field_name.as_str() {
                "avatar" => {
                    let _ = sqlx::query("UPDATE bands SET avatar_url = $1 WHERE id = $2")
                        .bind(&url).bind(band_id).execute(&pool).await;
                    result["avatar_url"] = serde_json::json!(url);
                }
                "banner" => {
                    let _ = sqlx::query("UPDATE bands SET banner_url = $1 WHERE id = $2")
                        .bind(&url).bind(band_id).execute(&pool).await;
                    result["banner_url"] = serde_json::json!(url);
                }
                _ => {}
            }
        }
    }
    Ok(Json(result))
}

// ── Add/Remove Members ──

async fn add_member(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(band_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let is_admin: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM band_members WHERE band_id = $1 AND user_id = $2 AND role = 'admin'"
    ).bind(band_id).bind(auth.0).fetch_one(&pool).await.unwrap_or(0) > 0;

    if !is_admin {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Only band admins can add members".into() })));
    }

    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or("");
    let role = body.get("role").and_then(|v| v.as_str()).unwrap_or("member");
    let instrument = body.get("instrument").and_then(|v| v.as_str()).unwrap_or("");

    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username).fetch_optional(&pool).await.ok().flatten();

    let uid = user_id.ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: format!("User '{username}' not found") })))?;

    let _ = sqlx::query(
        "INSERT INTO band_members (band_id, user_id, role, instrument) VALUES ($1,$2,$3,$4) ON CONFLICT (band_id, user_id) DO UPDATE SET role=$3, instrument=$4"
    ).bind(band_id).bind(uid).bind(role).bind(instrument).execute(&pool).await;

    Ok(Json(serde_json::json!({ "status": "added", "user_id": uid })))
}

async fn remove_member(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path((band_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let is_admin: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM band_members WHERE band_id = $1 AND user_id = $2 AND role = 'admin'"
    ).bind(band_id).bind(auth.0).fetch_one(&pool).await.unwrap_or(0) > 0;

    if !is_admin && auth.0 != user_id {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Only admins or the member themselves can remove".into() })));
    }

    let _ = sqlx::query("DELETE FROM band_members WHERE band_id = $1 AND user_id = $2")
        .bind(band_id).bind(user_id).execute(&pool).await;

    Ok(Json(serde_json::json!({ "status": "removed" })))
}

// ── Delete Band ──

async fn delete_band(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(band_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let band = sqlx::query_as::<_, Band>("SELECT * FROM bands WHERE id = $1")
        .bind(band_id).fetch_optional(&pool).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("DB: {e}") })))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Not found".into() })))?;

    if band.created_by != auth.0 {
        return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Only the band creator can delete it".into() })));
    }

    let _ = sqlx::query("DELETE FROM bands WHERE id = $1").bind(band_id).execute(&pool).await;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/bands", post(create_band).get(list_bands))
        .route("/bands/{id}", get(get_band).put(update_band).delete(delete_band))
        .route("/bands/{id}/images", post(upload_band_image))
        .route("/bands/{id}/members", post(add_member))
        .route("/bands/{band_id}/members/{user_id}", delete(remove_member))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024))
}
