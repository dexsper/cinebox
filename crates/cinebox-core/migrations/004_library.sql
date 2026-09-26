CREATE TABLE library (
    kind TEXT NOT NULL,
    id INTEGER NOT NULL,
    status TEXT,
    liked INTEGER NOT NULL DEFAULT 0,
    section TEXT NOT NULL,
    title TEXT NOT NULL,
    poster_path TEXT,
    year INTEGER,
    vote REAL,
    added_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
);

CREATE INDEX library_updated ON library(updated_at);

ALTER TABLE watch_history ADD COLUMN section TEXT;
