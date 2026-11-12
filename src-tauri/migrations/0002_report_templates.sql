-- User-designed report templates: presentation rules only (page, header, table, footer,
-- formats). A template never contains SQL, connection details, credentials or result data.
-- `definition` is the versioned template JSON; `schema_version` is copied out for migrations.
CREATE TABLE report_templates (
    id             TEXT    PRIMARY KEY NOT NULL,
    name           TEXT    NOT NULL,
    schema_version INTEGER NOT NULL,
    definition     TEXT    NOT NULL CHECK (json_valid(definition)),
    created_at     TEXT    NOT NULL,
    updated_at     TEXT    NOT NULL
);

CREATE INDEX report_templates_updated ON report_templates (updated_at DESC);
