use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::models::*;

/// Helper type for JSON error responses.
type ProjError = (StatusCode, Json<ErrorResponse>);

fn proj_err(status: StatusCode, msg: &str) -> ProjError {
    (status, Json(ErrorResponse { error: msg.into() }))
}

// ── Create project ──

async fn create_project(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Json(body): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, ProjError> {
    let is_public = body.is_public.unwrap_or(true);

    println!("[PROJECTS] Create project: title={:?}, owner={}", body.title, auth.0);

    let project = sqlx::query_as::<_, Project>(
        r#"INSERT INTO projects (owner_id, title, description, is_public)
           VALUES ($1, $2, $3, $4) RETURNING *"#,
    )
    .bind(auth.0)
    .bind(&body.title)
    .bind(body.description.as_deref().unwrap_or(""))
    .bind(is_public)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("create project error: {e}");
        proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}"))
    })?;

    Ok((StatusCode::CREATED, Json(project)))
}

// ── List projects ──

async fn list_projects(
    State(pool): State<PgPool>,
    Query(q): Query<ProjectQuery>,
) -> Result<impl IntoResponse, ProjError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let total: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::bigint FROM projects WHERE is_public = true")
        .fetch_one(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let projects = sqlx::query_as::<_, Project>(
        "SELECT * FROM projects WHERE is_public = true ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(Paginated {
        data: projects,
        page,
        per_page,
        total,
    }))
}

// ── Get project detail ──

async fn get_project(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ProjError> {
    let project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| proj_err(StatusCode::NOT_FOUND, "Project not found"))?;

    let owner =
        sqlx::query_as::<_, crate::models::User>("SELECT * FROM users WHERE id = $1")
            .bind(project.owner_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let tracks = sqlx::query_as::<_, ProjectTrack>(
        "SELECT * FROM project_tracks WHERE project_id = $1 ORDER BY added_at DESC",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(ProjectDetail {
        project,
        owner: owner.into(),
        tracks,
    }))
}

// ── Propose a track to a project ──

async fn propose_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<ProposeTrackRequest>,
) -> Result<impl IntoResponse, ProjError> {
    // Verify project exists
    let _project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| proj_err(StatusCode::NOT_FOUND, "Project not found"))?;

    // Verify track exists and belongs to the user
    let track = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = $1 AND user_id = $2")
        .bind(body.track_id)
        .bind(auth.0)
        .fetch_optional(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| proj_err(StatusCode::BAD_REQUEST, "Track not found or not owned by you"))?;

    let name = body
        .name
        .unwrap_or_else(|| track.title.clone());

    let pt = sqlx::query_as::<_, ProjectTrack>(
        r#"INSERT INTO project_tracks (project_id, user_id, track_id, name)
           VALUES ($1, $2, $3, $4) RETURNING *"#,
    )
    .bind(project_id)
    .bind(auth.0)
    .bind(body.track_id)
    .bind(&name)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("propose track error: {e}");
        proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}"))
    })?;

    Ok((StatusCode::CREATED, Json(pt)))
}

// ── Vote on a project track ──

async fn vote_track(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path((project_id, track_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<VoteRequest>,
) -> Result<impl IntoResponse, ProjError> {
    if body.vote != "up" && body.vote != "down" {
        return Err(proj_err(StatusCode::BAD_REQUEST, "Vote must be 'up' or 'down'"));
    }

    // Get the project_track
    let pt = sqlx::query_as::<_, ProjectTrack>(
        "SELECT * FROM project_tracks WHERE id = $1 AND project_id = $2",
    )
    .bind(track_id)
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
    .ok_or_else(|| proj_err(StatusCode::NOT_FOUND, "Project track not found"))?;

    // Check existing vote
    let existing = sqlx::query_as::<_, Vote>(
        "SELECT * FROM votes WHERE user_id = $1 AND project_track_id = $2",
    )
    .bind(auth.0)
    .bind(pt.id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    if let Some(old_vote) = existing {
        if old_vote.vote == body.vote {
            // Remove vote
            sqlx::query("DELETE FROM votes WHERE id = $1")
                .bind(old_vote.id)
                .execute(&pool)
                .await
                .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

            let col = if body.vote == "up" {
                "votes_up"
            } else {
                "votes_down"
            };
            sqlx::query(&format!(
                "UPDATE project_tracks SET {col} = {col} - 1 WHERE id = $1"
            ))
            .bind(pt.id)
            .execute(&pool)
            .await
            .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;
        } else {
            // Switch vote
            sqlx::query("UPDATE votes SET vote = $1 WHERE id = $2")
                .bind(&body.vote)
                .bind(old_vote.id)
                .execute(&pool)
                .await
                .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

            let (inc, dec) = if body.vote == "up" {
                ("votes_up", "votes_down")
            } else {
                ("votes_down", "votes_up")
            };
            sqlx::query(&format!(
                "UPDATE project_tracks SET {inc} = {inc} + 1, {dec} = {dec} - 1 WHERE id = $1"
            ))
            .bind(pt.id)
            .execute(&pool)
            .await
            .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;
        }
    } else {
        // New vote
        sqlx::query(
            "INSERT INTO votes (user_id, project_track_id, vote) VALUES ($1, $2, $3)",
        )
        .bind(auth.0)
        .bind(pt.id)
        .bind(&body.vote)
        .execute(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        let col = if body.vote == "up" {
            "votes_up"
        } else {
            "votes_down"
        };
        sqlx::query(&format!(
            "UPDATE project_tracks SET {col} = {col} + 1 WHERE id = $1"
        ))
        .bind(pt.id)
        .execute(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;
    }

    // Return updated project track
    let updated = sqlx::query_as::<_, ProjectTrack>(
        "SELECT * FROM project_tracks WHERE id = $1",
    )
    .bind(pt.id)
    .fetch_one(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(updated))
}

// ── Release a version ──

async fn release_version(
    State(pool): State<PgPool>,
    auth: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(body): Json<ReleaseRequest>,
) -> Result<impl IntoResponse, ProjError> {
    // Only owner can release
    let project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| proj_err(StatusCode::NOT_FOUND, "Project not found"))?;

    if project.owner_id != auth.0 {
        return Err(proj_err(StatusCode::FORBIDDEN, "Only the project owner can release versions"));
    }

    // Mark selected tracks as accepted
    for tid in &body.track_ids {
        sqlx::query("UPDATE project_tracks SET status = 'accepted' WHERE id = $1 AND project_id = $2")
            .bind(tid)
            .bind(project_id)
            .execute(&pool)
            .await
            .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;
    }

    let track_ids_json = serde_json::to_value(&body.track_ids)
        .map_err(|_| proj_err(StatusCode::INTERNAL_SERVER_ERROR, "Failed to serialize track IDs"))?;

    let version = sqlx::query_as::<_, ProjectVersion>(
        r#"INSERT INTO project_versions (project_id, name, description, track_ids, is_released)
           VALUES ($1, $2, $3, $4, true) RETURNING *"#,
    )
    .bind(project_id)
    .bind(&body.name)
    .bind(body.description.as_deref().unwrap_or(""))
    .bind(&track_ids_json)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("release error: {e}");
        proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}"))
    })?;

    Ok((StatusCode::CREATED, Json(version)))
}

// ── List versions ──

async fn list_versions(
    State(pool): State<PgPool>,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ProjError> {
    let versions = sqlx::query_as::<_, ProjectVersion>(
        "SELECT * FROM project_versions WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    Ok(Json(versions))
}

// ── Checkout project (for DAW integration) ──

async fn checkout_project(
    State(pool): State<PgPool>,
    _auth: AuthUser,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, ProjError> {
    println!("[PROJECTS] Checkout project: {project_id}");

    let project = sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?
        .ok_or_else(|| proj_err(StatusCode::NOT_FOUND, "Project not found"))?;

    let pt_list = sqlx::query_as::<_, ProjectTrack>(
        "SELECT * FROM project_tracks WHERE project_id = $1 AND status = 'accepted'",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

    let mut track_files = Vec::new();
    for pt in &pt_list {
        let track = sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = $1")
            .bind(pt.track_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| proj_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {e}")))?;

        if let Some(t) = track {
            track_files.push(TrackFile {
                track_id: t.id,
                name: pt.name.clone(),
                audio_url: t.audio_url.clone(),
                status: pt.status.clone(),
            });
        }
    }

    Ok(Json(CheckoutResponse {
        project,
        tracks: track_files,
    }))
}

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/projects", post(create_project).get(list_projects))
        .route("/projects/{id}", get(get_project))
        .route("/projects/{id}/tracks", post(propose_track))
        .route(
            "/projects/{id}/tracks/{track_id}/vote",
            post(vote_track),
        )
        .route("/projects/{id}/release", post(release_version))
        .route("/projects/{id}/versions", get(list_versions))
        .route("/projects/{id}/checkout", post(checkout_project))
}
