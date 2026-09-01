use anyhow::Result;
use rusqlite::{params, Connection};

use crate::decision::rules::{Action, DecisionOutcome};
use crate::model::sensor::SensorReading;
use crate::twin::ZoneTwinState;

#[cfg(feature = "dashboard")]
use crate::model::command::ValveAction;
#[cfg(feature = "dashboard")]
use serde::Serialize;

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
             valve_1_open, valve_2_open, valve_1_source, valve_1_expires_at, valve_2_source, valve_2_expires_at,
             water_balance_mm, last_reading_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(device_id) DO UPDATE SET
            last_soil_moisture_1_pct = excluded.last_soil_moisture_1_pct,
            last_soil_moisture_2_pct = excluded.last_soil_moisture_2_pct,
            last_temp_1_c = excluded.last_temp_1_c,
            last_temp_2_c = excluded.last_temp_2_c,
            valve_1_open = excluded.valve_1_open,
            valve_2_open = excluded.valve_2_open,
            valve_1_source = excluded.valve_1_source,
            valve_1_expires_at = excluded.valve_1_expires_at,
            valve_2_source = excluded.valve_2_source,
            valve_2_expires_at = excluded.valve_2_expires_at,
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
            state.valve_1_source,
            state.valve_1_expires_at,
            state.valve_2_source,
            state.valve_2_expires_at,
            state.water_balance_mm,
            state.last_reading_at,
            updated_at,
        ],
    )?;
    Ok(())
}

#[cfg(feature = "dashboard")]
#[derive(Debug, Serialize)]
pub struct TwinStateRow {
    pub device_id: String,
    pub last_soil_moisture_1_pct: Option<f32>,
    pub last_soil_moisture_2_pct: Option<f32>,
    pub last_temp_1_c: Option<f32>,
    pub last_temp_2_c: Option<f32>,
    pub valve_1_open: bool,
    pub valve_2_open: bool,
    pub valve_1_source: String,
    pub valve_1_expires_at: Option<i64>,
    pub valve_2_source: String,
    pub valve_2_expires_at: Option<i64>,
    pub water_balance_mm: f32,
    pub last_reading_at: Option<i64>,
    pub updated_at: i64,
}

#[cfg(feature = "dashboard")]
#[derive(Debug, Serialize)]
pub struct ReadingRow {
    pub id: i64,
    pub device_id: String,
    pub seq: Option<i64>,
    pub uptime_ms: Option<i64>,
    pub soil_moisture_1_pct: Option<f32>,
    pub soil_moisture_2_pct: Option<f32>,
    pub temp_1_c: Option<f32>,
    pub temp_2_c: Option<f32>,
    pub received_at: i64,
}

#[cfg(feature = "dashboard")]
#[derive(Debug, Serialize)]
pub struct DecisionRow {
    pub id: i64,
    pub device_id: String,
    pub valve_id: u8,
    pub command_id: Option<String>,
    pub action: String,
    pub duration_s: Option<i64>,
    pub reason: Option<String>,
    pub trigger_soil_moisture_pct: Option<f32>,
    pub trigger_temp_c: Option<f32>,
    pub threshold_used: Option<f32>,
    pub published: bool,
    pub created_at: i64,
}

#[cfg(feature = "dashboard")]
pub fn list_twin_states(conn: &Connection) -> Result<Vec<TwinStateRow>> {
    let mut stmt = conn.prepare(
        "SELECT device_id, last_soil_moisture_1_pct, last_soil_moisture_2_pct,
                last_temp_1_c, last_temp_2_c, valve_1_open, valve_2_open,
                valve_1_source, valve_1_expires_at, valve_2_source, valve_2_expires_at,
                water_balance_mm, last_reading_at, updated_at
         FROM twin_state ORDER BY device_id",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TwinStateRow {
                device_id: row.get(0)?,
                last_soil_moisture_1_pct: row.get(1)?,
                last_soil_moisture_2_pct: row.get(2)?,
                last_temp_1_c: row.get(3)?,
                last_temp_2_c: row.get(4)?,
                valve_1_open: row.get(5)?,
                valve_2_open: row.get(6)?,
                valve_1_source: row.get(7)?,
                valve_1_expires_at: row.get(8)?,
                valve_2_source: row.get(9)?,
                valve_2_expires_at: row.get(10)?,
                water_balance_mm: row.get(11)?,
                last_reading_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(feature = "dashboard")]
pub fn list_readings(conn: &Connection, device_id: &str, limit: i64) -> Result<Vec<ReadingRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, device_id, seq, uptime_ms, soil_moisture_1_pct, soil_moisture_2_pct,
                temp_1_c, temp_2_c, received_at
         FROM readings WHERE device_id = ?1 ORDER BY received_at DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![device_id, limit], |row| {
            Ok(ReadingRow {
                id: row.get(0)?,
                device_id: row.get(1)?,
                seq: row.get(2)?,
                uptime_ms: row.get(3)?,
                soil_moisture_1_pct: row.get(4)?,
                soil_moisture_2_pct: row.get(5)?,
                temp_1_c: row.get(6)?,
                temp_2_c: row.get(7)?,
                received_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(feature = "dashboard")]
pub fn list_decisions(conn: &Connection, device_id: &str, limit: i64) -> Result<Vec<DecisionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, device_id, valve_id, command_id, action, duration_s, reason,
                trigger_soil_moisture_pct, trigger_temp_c, threshold_used, published, created_at
         FROM decisions WHERE device_id = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![device_id, limit], |row| {
            Ok(DecisionRow {
                id: row.get(0)?,
                device_id: row.get(1)?,
                valve_id: row.get(2)?,
                command_id: row.get(3)?,
                action: row.get(4)?,
                duration_s: row.get(5)?,
                reason: row.get(6)?,
                trigger_soil_moisture_pct: row.get(7)?,
                trigger_temp_c: row.get(8)?,
                threshold_used: row.get(9)?,
                published: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Catat command manual dari dashboard. Terpisah dari `insert_decision` karena
/// command manual tidak punya nilai trigger/threshold (bukan hasil evaluasi engine).
#[cfg(feature = "dashboard")]
#[allow(clippy::too_many_arguments)]
pub fn insert_manual_decision(
    conn: &Connection,
    device_id: &str,
    valve_id: u8,
    action: ValveAction,
    duration_s: u32,
    command_id: &str,
    published: bool,
    created_at: i64,
) -> Result<()> {
    let action_str = match action {
        ValveAction::Open => "open",
        ValveAction::Close => "close",
    };
    conn.execute(
        "INSERT INTO decisions
            (device_id, valve_id, command_id, action, duration_s, reason,
             trigger_soil_moisture_pct, trigger_temp_c, threshold_used, published, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'manual_override', NULL, NULL, NULL, ?6, ?7)",
        params![
            device_id,
            valve_id,
            command_id,
            action_str,
            duration_s,
            published as i64,
            created_at,
        ],
    )?;
    Ok(())
}

/// Update optimistic valve state di twin_state setelah manual override berhasil
/// dipublish -- source of truth tetap reading asli berikutnya dari loop MQTT.
#[cfg(feature = "dashboard")]
#[allow(clippy::too_many_arguments)]
pub fn update_twin_valve_state(
    conn: &Connection,
    device_id: &str,
    valve_id: u8,
    open: bool,
    source: &str,
    expires_at: Option<i64>,
    updated_at: i64,
) -> Result<()> {
    let sql = if valve_id == 1 {
        "UPDATE twin_state SET valve_1_open = ?2, valve_1_source = ?3, valve_1_expires_at = ?4, updated_at = ?5 WHERE device_id = ?1"
    } else {
        "UPDATE twin_state SET valve_2_open = ?2, valve_2_source = ?3, valve_2_expires_at = ?4, updated_at = ?5 WHERE device_id = ?1"
    };
    conn.execute(sql, params![device_id, open as i64, source, expires_at, updated_at])?;
    Ok(())
}

/// Kembalikan valve ke mode "auto" tanpa mengubah status buka/tutup -- dipakai tombol
/// "Lepas ke Otomatis" di dashboard. Tidak publish MQTT (bukan actuator command).
#[cfg(feature = "dashboard")]
pub fn release_valve_to_auto(
    conn: &Connection,
    device_id: &str,
    valve_id: u8,
    updated_at: i64,
) -> Result<()> {
    let sql = if valve_id == 1 {
        "UPDATE twin_state SET valve_1_source = 'auto', valve_1_expires_at = NULL, updated_at = ?2 WHERE device_id = ?1"
    } else {
        "UPDATE twin_state SET valve_2_source = 'auto', valve_2_expires_at = NULL, updated_at = ?2 WHERE device_id = ?1"
    };
    conn.execute(sql, params![device_id, updated_at])?;
    Ok(())
}
