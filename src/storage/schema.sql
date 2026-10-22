CREATE TABLE IF NOT EXISTS readings (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id            TEXT NOT NULL,
    seq                  INTEGER,
    uptime_ms            INTEGER,
    soil_moisture_1_pct  REAL,
    soil_moisture_2_pct  REAL,
    temp_1_c             REAL,
    temp_2_c             REAL,
    received_at          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_readings_device_time ON readings(device_id, received_at);

CREATE TABLE IF NOT EXISTS twin_state (
    device_id                 TEXT PRIMARY KEY,
    last_soil_moisture_1_pct  REAL,
    last_soil_moisture_2_pct  REAL,
    last_temp_1_c             REAL,
    last_temp_2_c             REAL,
    valve_1_open              INTEGER NOT NULL DEFAULT 0,
    valve_2_open              INTEGER NOT NULL DEFAULT 0,
    valve_1_source            TEXT NOT NULL DEFAULT 'auto',
    valve_1_expires_at        INTEGER,
    valve_2_source            TEXT NOT NULL DEFAULT 'auto',
    valve_2_expires_at        INTEGER,
    last_reading_at           INTEGER,
    updated_at                INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS decisions (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id                   TEXT NOT NULL,
    valve_id                    INTEGER NOT NULL,
    command_id                  TEXT,
    action                      TEXT NOT NULL,
    duration_s                  INTEGER,
    reason                      TEXT,
    trigger_soil_moisture_pct   REAL,
    trigger_temp_c              REAL,
    threshold_used              REAL,
    published                   INTEGER NOT NULL DEFAULT 0,
    created_at                  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_decisions_device_time ON decisions(device_id, created_at);

CREATE TABLE IF NOT EXISTS command_acks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    command_id    TEXT NOT NULL,
    device_id     TEXT NOT NULL,
    status        TEXT,
    received_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS node_settings (
    device_id   TEXT PRIMARY KEY,
    zone        TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS valve_settings (
    device_id    TEXT NOT NULL,
    valve_id     INTEGER NOT NULL CHECK (valve_id IN (1, 2)),
    config_json  TEXT NOT NULL,
    mode         TEXT GENERATED ALWAYS AS (json_extract(config_json, '$.mode')) VIRTUAL,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (device_id, valve_id)
);
