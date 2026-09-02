CREATE TABLE tmdb_cache (
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (language, kind, id)
);

CREATE TABLE tmdb_image (
    size TEXT NOT NULL,
    path TEXT NOT NULL,
    fetched_at INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,
    bytes BLOB NOT NULL,
    PRIMARY KEY (size, path)
);

CREATE TABLE tmdb_image_ref (
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    path TEXT NOT NULL,
    PRIMARY KEY (language, kind, id, path)
);

CREATE INDEX tmdb_image_ref_path ON tmdb_image_ref(path);
CREATE INDEX tmdb_image_accessed ON tmdb_image(accessed_at);

CREATE TABLE torrent_playback_prefs (
    hash TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE watch_timeline (
    kind TEXT NOT NULL,
    id INTEGER NOT NULL,
    season INTEGER NOT NULL DEFAULT -1,
    episode INTEGER NOT NULL DEFAULT -1,
    time REAL NOT NULL,
    duration REAL NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (kind, id, season, episode)
);

CREATE TABLE watch_history (
    kind TEXT NOT NULL,
    id INTEGER NOT NULL,
    title TEXT NOT NULL,
    poster_path TEXT,
    year INTEGER,
    vote REAL,
    season INTEGER,
    episode INTEGER,
    episode_title TEXT,
    time REAL NOT NULL,
    duration REAL NOT NULL,
    last_hash TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
);

CREATE INDEX watch_history_updated ON watch_history(updated_at);
