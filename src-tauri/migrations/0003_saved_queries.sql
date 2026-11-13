-- Saved SQL Editor queries. `variables` is the JSON array of variable definitions (filters).
-- A saved query never contains credentials or result data, and is not tied to a connection.
CREATE TABLE saved_queries (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sql_text    TEXT NOT NULL,
    variables   TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(variables) AND json_type(variables) = 'array'),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX saved_queries_updated ON saved_queries (updated_at DESC);
