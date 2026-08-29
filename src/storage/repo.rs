use anyhow::Result;
use rusqlite::{params, Connection};

use crate::decision::rules::{Action, DecisionOutcome};
use crate::model::sensor::SensorReading;
use crate::twin::ZoneTwinState;

pub fn insert_reading(conn: &Connection, reading: &SensorReading, received_at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO readings
            (device_id, seq, uptime_ms, soil_moisture_1_pct, soil_moisture_2_pct, temp_1_c, temp_2_c, received_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            reading.device_id,
            reading.seq,
            reading.uptime_ms,
            reading.soil_moisture_1_pct,
            reading.soil_moisture_2_pct,
            reading.temp_1_c,
            reading.temp_2_c,
            received_at,
        ],
    )?;
    Ok(())
}

pub fn insert_decision(
    conn: &Connection,
    device_id: &str,
    command_id: Option<&str>,
    outcome: &DecisionOutcome,
    published: bool,
    created_at: i64,
) -> Result<()> {
    let action_str = match outcome.action {
        Action::Open => "open",
        Action::Close => "close",
        Action::None => "none",
    };
    conn.execute(
        "INSERT INTO decisions
            (device_id, valve_id, command_id, action, duration_s, reason,
             trigger_soil_moisture_pct, trigger_temp_c, threshold_used, published, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            device_id,
            outcome.valve_id,
            command_id,
            action_str,
            outcome.duration_s,
            outcome.reason,
            outcome.trigger_soil_moisture_pct,
            outcome.trigger_temp_c,
            outcome.threshold_used,
            published as i64,
            created_at,
        ],
    )?;
    Ok(())
}

pub fn upsert_twin_state(
    conn: &Connection,
    device_id: &str,
    state: &ZoneTwinState,
    updated_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO twin_state
            (device_id, last_soil_moisture_1_pct, last_soil_moisture_2_pct, last_temp_1_c, last_temp_2_c,
             valve_1_open, valve_2_open, water_balance_mm, last_reading_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(device_id) DO UPDATE SET
            last_soil_moisture_1_pct = excluded.last_soil_moisture_1_pct,
            last_soil_moisture_2_pct = excluded.last_soil_moisture_2_pct,
            last_temp_1_c = excluded.last_temp_1_c,
            last_temp_2_c = excluded.last_temp_2_c,
            valve_1_open = excluded.valve_1_open,
            valve_2_open = excluded.valve_2_open,
            water_balance_mm = excluded.water_balance_mm,
            last_reading_at = excluded.last_reading_at,
            updated_at = excluded.updated_at",
        params![
            device_id,
            state.last_soil_moisture_1_pct,
            state.last_soil_moisture_2_pct,
            state.last_temp_1_c,
            state.last_temp_2_c,
            state.valve_1_open as i64,
            state.valve_2_open as i64,
            state.water_balance_mm,
            state.last_reading_at,
            updated_at,
        ],
    )?;
    Ok(())
}
