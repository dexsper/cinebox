CREATE TABLE skip_segment_choice (
    kind TEXT NOT NULL,
    tmdb_id INTEGER NOT NULL,
    segment_type TEXT NOT NULL,
    armed INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (kind, tmdb_id, segment_type)
);
