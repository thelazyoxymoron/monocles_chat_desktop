-- App-wide key/value settings (e.g. "auto_trust_new_keys").
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
