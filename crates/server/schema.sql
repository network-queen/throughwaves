-- ThroughWaves Web Platform Schema

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Users
CREATE TABLE IF NOT EXISTS users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username    VARCHAR(64) UNIQUE NOT NULL,
    email       VARCHAR(255) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    avatar_url  TEXT,
    bio         TEXT DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Tracks
CREATE TABLE IF NOT EXISTS tracks (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title            VARCHAR(255) NOT NULL,
    description      TEXT DEFAULT '',
    audio_url        TEXT NOT NULL,
    waveform_data    JSONB,
    duration_seconds DOUBLE PRECISION DEFAULT 0,
    genre            VARCHAR(64) DEFAULT '',
    bpm              INTEGER,
    key              VARCHAR(8) DEFAULT '',
    plays            BIGINT DEFAULT 0,
    likes            BIGINT DEFAULT 0,
    is_public        BOOLEAN DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_tracks_user ON tracks(user_id);
CREATE INDEX idx_tracks_genre ON tracks(genre);
CREATE INDEX idx_tracks_created ON tracks(created_at DESC);

-- Track likes (many-to-many)
CREATE TABLE IF NOT EXISTS track_likes (
    user_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, track_id)
);

-- Comments (timed comments on tracks)
CREATE TABLE IF NOT EXISTS comments (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id           UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id          UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    text              TEXT NOT NULL,
    timestamp_seconds DOUBLE PRECISION,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_comments_track ON comments(track_id);

-- Projects (collaborative)
CREATE TABLE IF NOT EXISTS projects (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       VARCHAR(255) NOT NULL,
    description TEXT DEFAULT '',
    is_public   BOOLEAN DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_projects_owner ON projects(owner_id);

-- Project tracks (proposed stems / contributions)
CREATE TABLE IF NOT EXISTS project_tracks (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id    UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    name        VARCHAR(255) NOT NULL DEFAULT '',
    status      VARCHAR(16) NOT NULL DEFAULT 'proposed' CHECK (status IN ('proposed','accepted','rejected')),
    votes_up    INTEGER DEFAULT 0,
    votes_down  INTEGER DEFAULT 0,
    added_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_project_tracks_project ON project_tracks(project_id);

-- Votes on project tracks
CREATE TABLE IF NOT EXISTS votes (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_track_id UUID NOT NULL REFERENCES project_tracks(id) ON DELETE CASCADE,
    vote             VARCHAR(4) NOT NULL CHECK (vote IN ('up','down')),
    UNIQUE (user_id, project_track_id)
);

-- Project versions (released mixes)
CREATE TABLE IF NOT EXISTS project_versions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        VARCHAR(255) NOT NULL,
    description TEXT DEFAULT '',
    track_ids   JSONB NOT NULL DEFAULT '[]',
    is_released BOOLEAN DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_project_versions_project ON project_versions(project_id);

-- Follows (user follow system)
CREATE TABLE IF NOT EXISTS follows (
    follower_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    following_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (follower_id, following_id),
    CHECK (follower_id != following_id)
);

CREATE INDEX idx_follows_follower ON follows(follower_id);
CREATE INDEX idx_follows_following ON follows(following_id);

-- Reposts (track repost system)
CREATE TABLE IF NOT EXISTS reposts (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id   UUID NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, track_id)
);

CREATE INDEX idx_reposts_user ON reposts(user_id);
CREATE INDEX idx_reposts_track ON reposts(track_id);

-- Cloud Projects (full DAW project with stems)
CREATE TABLE IF NOT EXISTS cloud_projects (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title            VARCHAR(255) NOT NULL,
    description      TEXT DEFAULT '',
    mixdown_url      TEXT NOT NULL,
    project_data     JSONB,
    waveform_data    JSONB,
    duration_seconds DOUBLE PRECISION DEFAULT 0,
    genre            VARCHAR(64) DEFAULT '',
    bpm              INTEGER,
    plays            BIGINT DEFAULT 0,
    is_public        BOOLEAN DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_cloud_projects_user ON cloud_projects(user_id);

-- Cloud Project Stems (individual tracks within a cloud project)
CREATE TABLE IF NOT EXISTS cloud_project_stems (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cloud_project_id  UUID NOT NULL REFERENCES cloud_projects(id) ON DELETE CASCADE,
    name              VARCHAR(255) NOT NULL,
    audio_url         TEXT NOT NULL,
    track_index       INTEGER NOT NULL DEFAULT 0,
    kind              VARCHAR(16) DEFAULT 'audio',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_cloud_stems_project ON cloud_project_stems(cloud_project_id);

-- Add content hash to stems for delta detection
ALTER TABLE cloud_project_stems ADD COLUMN IF NOT EXISTS content_hash VARCHAR(64) DEFAULT '';

-- Cloud Project Versions (git-like history)
CREATE TABLE IF NOT EXISTS cloud_project_versions (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cloud_project_id  UUID NOT NULL REFERENCES cloud_projects(id) ON DELETE CASCADE,
    version_number    INTEGER NOT NULL DEFAULT 1,
    message           TEXT DEFAULT '',
    mixdown_url       TEXT NOT NULL,
    waveform_data     JSONB,
    duration_seconds  DOUBLE PRECISION DEFAULT 0,
    -- JSON array of stem references: [{stem_id, name, track_index, is_new}]
    stem_refs         JSONB NOT NULL DEFAULT '[]',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_cloud_versions_project ON cloud_project_versions(cloud_project_id);

-- Link cloud projects to their published track
ALTER TABLE cloud_projects ADD COLUMN IF NOT EXISTS published_track_id UUID REFERENCES tracks(id) ON DELETE SET NULL;

-- Admin role
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_admin BOOLEAN DEFAULT FALSE;
UPDATE users SET is_admin = true WHERE email = 'klymenko.ruslan.dev@gmail.com';
