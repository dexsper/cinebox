CREATE TABLE torrserver_match (
    hash TEXT NOT NULL PRIMARY KEY,
    kind TEXT,
    id INTEGER,
    checked_at INTEGER NOT NULL
);

CREATE INDEX torrserver_match_title ON torrserver_match(kind, id);

CREATE TABLE torrserver_viewed_seen (
    hash TEXT NOT NULL,
    file_index INTEGER NOT NULL,
    timecode REAL NOT NULL,
    PRIMARY KEY (hash, file_index)
);

CREATE TABLE torrserver_origin (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    url TEXT NOT NULL
);
