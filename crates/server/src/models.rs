use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ── Error Response ──

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

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
    pub is_admin: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_admin: Option<bool>,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            avatar_url: u.avatar_url,
            bio: u.bio,
            created_at: u.created_at,
            is_admin: u.is_admin,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
    pub follower_count: i64,
    pub following_count: i64,
    pub track_count: i64,
    pub is_following: bool,
}

// ── Follow ──

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Follow {
    pub follower_id: Uuid,
    pub following_id: Uuid,
    pub created_at: DateTime<Utc>,
}

// ── Repost ──

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Repost {
    pub user_id: Uuid,
    pub track_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct TrackWithRepost {
    #[serde(flatten)]
    pub track: Track,
    pub reposted_by: Option<String>,
    pub repost_count: i64,
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

// ── Cloud Projects ──

// ── Bands ──

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Band {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub genre: Option<String>,
    pub avatar_url: Option<String>,
    pub banner_url: Option<String>,
    pub website: Option<String>,
    pub location: Option<String>,
    pub created_by: Uuid,
    pub is_public: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub likes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BandMember {
    pub id: Uuid,
    pub band_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub instrument: Option<String>,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CloudProject {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub mixdown_url: String,
    pub project_data: Option<serde_json::Value>,
    pub waveform_data: Option<serde_json::Value>,
    pub duration_seconds: Option<f64>,
    pub genre: Option<String>,
    pub bpm: Option<i32>,
    pub plays: Option<i64>,
    pub is_public: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub published_track_id: Option<Uuid>,
    pub band_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CloudProjectStem {
    pub id: Uuid,
    pub cloud_project_id: Uuid,
    pub name: String,
    pub audio_url: String,
    pub track_index: i32,
    pub kind: Option<String>,
    pub created_at: DateTime<Utc>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CloudProjectVersion {
    pub id: Uuid,
    pub cloud_project_id: Uuid,
    pub version_number: i32,
    pub message: Option<String>,
    pub mixdown_url: String,
    pub waveform_data: Option<serde_json::Value>,
    pub duration_seconds: Option<f64>,
    pub stem_refs: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
