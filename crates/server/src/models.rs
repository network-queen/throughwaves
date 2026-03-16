use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ── User ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            avatar_url: u.avatar_url,
            bio: u.bio,
            created_at: u.created_at,
        }
    }
}

// ── Track ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Track {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub audio_url: String,
    pub waveform_data: Option<serde_json::Value>,
    pub duration_seconds: Option<f64>,
    pub genre: Option<String>,
    pub bpm: Option<i32>,
    pub key: Option<String>,
    pub plays: Option<i64>,
    pub likes: Option<i64>,
    pub is_public: Option<bool>,
    pub created_at: DateTime<Utc>,
}

// ── Comment ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Comment {
    pub id: Uuid,
    pub user_id: Uuid,
    pub track_id: Uuid,
    pub text: String,
    pub timestamp_seconds: Option<f64>,
    pub created_at: DateTime<Utc>,
}

// ── Project ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub is_public: Option<bool>,
    pub created_at: DateTime<Utc>,
}

// ── ProjectTrack ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectTrack {
    pub id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub track_id: Uuid,
    pub name: String,
    pub status: String,
    pub votes_up: Option<i32>,
    pub votes_down: Option<i32>,
    pub added_at: DateTime<Utc>,
}

// ── Vote ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Vote {
    pub id: Uuid,
    pub user_id: Uuid,
    pub project_track_id: Uuid,
    pub vote: String,
}

// ── ProjectVersion ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectVersion {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub track_ids: serde_json::Value,
    pub is_released: Option<bool>,
    pub created_at: DateTime<Utc>,
}

// ── Request / Response DTOs ──

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserPublic,
}

#[derive(Debug, Deserialize)]
pub struct TrackQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub genre: Option<String>,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct TrackDetail {
    pub track: Track,
    pub user: UserPublic,
    pub comments: Vec<CommentWithUser>,
}

#[derive(Debug, Serialize)]
pub struct CommentWithUser {
    pub id: Uuid,
    pub user: UserPublic,
    pub text: String,
    pub timestamp_seconds: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CommentRequest {
    pub text: String,
    pub timestamp_seconds: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub title: String,
    pub description: Option<String>,
    pub is_public: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ProposeTrackRequest {
    pub track_id: Uuid,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VoteRequest {
    pub vote: String,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseRequest {
    pub name: String,
    pub description: Option<String>,
    pub track_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ProjectDetail {
    pub project: Project,
    pub owner: UserPublic,
    pub tracks: Vec<ProjectTrack>,
}

#[derive(Debug, Serialize)]
pub struct Paginated<T: Serialize> {
    pub data: Vec<T>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct ProjectQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub project: Project,
    pub tracks: Vec<TrackFile>,
}

#[derive(Debug, Serialize)]
pub struct TrackFile {
    pub track_id: Uuid,
    pub name: String,
    pub audio_url: String,
    pub status: String,
}
