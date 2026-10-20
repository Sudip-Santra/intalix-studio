-- Saved PostgreSQL connections. Never contains passwords: those live in Windows
-- Credential Manager, referenced by profile id.
CREATE TABLE connection_profiles (
    id                   TEXT    PRIMARY KEY NOT NULL,
    name                 TEXT    NOT NULL,
    environment          TEXT    NOT NULL CHECK (environment IN ('development', 'staging', 'production')),
    host                 TEXT    NOT NULL,
    port                 INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    database_name        TEXT    NOT NULL,
    username             TEXT    NOT NULL,
    ssl_mode             TEXT    NOT NULL CHECK (ssl_mode IN ('disable', 'prefer', 'require', 'verify-ca', 'verify-full')),
    ca_cert_path         TEXT    NOT NULL DEFAULT '',
    connect_timeout_secs INTEGER NOT NULL,
    query_timeout_secs   INTEGER NOT NULL,
    max_connections      INTEGER NOT NULL,
    application_name     TEXT    NOT NULL DEFAULT '',
    has_stored_password  INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT    NOT NULL,
    updated_at           TEXT    NOT NULL,
    last_connected_at    TEXT
);

CREATE INDEX connection_profiles_last_used ON connection_profiles (last_connected_at DESC, created_at DESC);
