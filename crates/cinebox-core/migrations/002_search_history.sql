CREATE TABLE search_history (
    query TEXT NOT NULL COLLATE NOCASE,
    searched_at INTEGER NOT NULL,
    PRIMARY KEY (query)
);

CREATE INDEX search_history_searched ON search_history(searched_at);
