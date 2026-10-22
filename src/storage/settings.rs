use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use tracing::warn;

use crate::config::{NodeConfig, ValveConfig};

pub fn seed_from_config(conn: &mut Connection, nodes: &[NodeConfig]) -> Result<bool> {
    let existing: i64 = conn.query_row("SELECT COUNT(*) FROM node_settings", [], |r| r.get(0))?;
    if existing > 0 {
        return Ok(false);
    }
    let tx = conn.transaction()?;
    let now = chrono::Utc::now().timestamp();
    for node in nodes {
        upsert_node_tx(&tx, node, now)?;
    }
    tx.commit()?;
    Ok(true)
}

fn upsert_node_tx(tx: &rusqlite::Transaction, node: &NodeConfig, now: i64) -> Result<()> {
    node.validate().map_err(|e| anyhow::anyhow!(e))?;
    tx.execute(
        "INSERT INTO node_settings (device_id, zone, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(device_id) DO UPDATE SET zone=excluded.zone, enabled=excluded.enabled, updated_at=excluded.updated_at",
        params![node.id, node.zone, node.enabled as i64, now, now],
    )?;
    for (valve_id, cfg) in [(1, &node.valve_1), (2, &node.valve_2)] {
        let json = serde_json::to_string(cfg)?;
        tx.execute(
            "INSERT INTO valve_settings (device_id, valve_id, config_json, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(device_id, valve_id) DO UPDATE SET config_json=excluded.config_json, updated_at=excluded.updated_at",
            params![node.id, valve_id, json, now],
        )?;
    }
    Ok(())
}

pub fn upsert_node(conn: &mut Connection, node: &NodeConfig) -> Result<()> {
    node.validate().map_err(|e| anyhow::anyhow!(e))?;
    let now = chrono::Utc::now().timestamp();
    let tx = conn.transaction()?;
    upsert_node_tx(&tx, node, now)?;
    tx.commit()?;
    Ok(())
}

pub fn load_nodes(conn: &Connection) -> Result<Vec<NodeConfig>> {
    let mut stmt = conn.prepare("SELECT device_id, zone, enabled FROM node_settings ORDER BY device_id")?;
    let node_rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;

    let mut nodes = Vec::new();
    for (device_id, zone, enabled) in node_rows {
        let mut configs: [Option<ValveConfig>; 2] = [None, None];
        for valve_id in [1, 2] {
            let json: Option<String> = conn.query_row(
                "SELECT config_json FROM valve_settings WHERE device_id=?1 AND valve_id=?2",
                params![device_id, valve_id],
                |r| r.get(0),
            ).optional()?;
            if let Some(j) = json {
                match serde_json::from_str::<ValveConfig>(&j) {
                    Ok(cfg) => configs[(valve_id - 1) as usize] = Some(cfg),
                    Err(e) => {
                        warn!("config_json rusak untuk {device_id} valve {valve_id}: {e}, pakai default");
                        configs[(valve_id - 1) as usize] = Some(default_valve_config());
                    }
                }
            } else {
                configs[(valve_id - 1) as usize] = Some(default_valve_config());
            }
        }
        nodes.push(NodeConfig {
            id: device_id,
            zone,
            valve_1: configs[0].clone().unwrap(),
            valve_2: configs[1].clone().unwrap(),
            enabled: enabled != 0,
        });
    }
    Ok(nodes)
}

fn default_valve_config() -> ValveConfig {
    ValveConfig::Schedule {
        close_temp_threshold_c: 25.0,
        close_soil_min_pct: 45.0,
        max_open_duration_s: 120,
        schedule: vec![],
    }
}

pub fn set_node_enabled(conn: &Connection, device_id: &str, enabled: bool) -> Result<bool> {
    let now = chrono::Utc::now().timestamp();
    let n = conn.execute("UPDATE node_settings SET enabled=?2, updated_at=?3 WHERE device_id=?1", params![device_id, enabled as i64, now])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScheduleWindow;
    use rusqlite::Connection;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn
    }

    fn sample_nodes() -> Vec<NodeConfig> {
        vec![NodeConfig {
            id: "esp32-01".to_string(),
            zone: Some("Zona A".to_string()),
            valve_1: ValveConfig::Schedule {
                close_temp_threshold_c: 25.0,
                close_soil_min_pct: 45.0,
                max_open_duration_s: 120,
                schedule: vec![ScheduleWindow { start: "06:00".to_string(), end: "06:15".to_string() }],
            },
            valve_2: ValveConfig::Schedule {
                close_temp_threshold_c: 25.0,
                close_soil_min_pct: 45.0,
                max_open_duration_s: 120,
                schedule: vec![],
            },
            enabled: true,
        }]
    }

    #[test]
    fn seed_mengisi_tabel_saat_kosong() {
        let mut conn = test_conn();
        let nodes = sample_nodes();
        let seeded = seed_from_config(&mut conn, &nodes).unwrap();
        assert!(seeded);
        let loaded = load_nodes(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "esp32-01");
    }

    #[test]
    fn seed_tidak_menimpa_saat_tabel_sudah_ada_isi() {
        let mut conn = test_conn();
        let nodes = sample_nodes();
        seed_from_config(&mut conn, &nodes).unwrap();
        let mut modified = nodes[0].clone();
        modified.zone = Some("Zona Z".to_string());
        upsert_node(&mut conn, &modified).unwrap();
        let nodes2 = vec![NodeConfig { id: "esp32-99".to_string(), zone: None, valve_1: modified.valve_1.clone(), valve_2: modified.valve_2.clone(), enabled: true }];
        let seeded2 = seed_from_config(&mut conn, &nodes2).unwrap();
        assert!(!seeded2);
        let loaded = load_nodes(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].zone.as_deref(), Some("Zona Z"));
    }

    #[test]
    fn load_nodes_round_trip_dengan_dua_window() {
        let mut conn = test_conn();
        let nodes = sample_nodes();
        seed_from_config(&mut conn, &nodes).unwrap();
        let loaded = load_nodes(&conn).unwrap();
        if let ValveConfig::Schedule { schedule, .. } = &loaded[0].valve_1 {
            assert_eq!(schedule.len(), 1);
            assert_eq!(schedule[0].start, "06:00");
        } else { panic!("expected schedule"); }
    }

    #[test]
    fn load_nodes_pakai_default_aman_saat_config_json_rusak() {
        let mut conn = test_conn();
        let nodes = sample_nodes();
        seed_from_config(&mut conn, &nodes).unwrap();
        conn.execute("UPDATE valve_settings SET config_json='{\"mode\":\"unknown\"}' WHERE device_id='esp32-01' AND valve_id=1", []).unwrap();
        let loaded = load_nodes(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn nonaktifkan_node_tidak_menghapus_readings() {
        let mut conn = test_conn();
        let nodes = sample_nodes();
        seed_from_config(&mut conn, &nodes).unwrap();
        conn.execute("INSERT INTO readings (device_id, received_at) VALUES ('esp32-01', 123)", []).unwrap();
        set_node_enabled(&conn, "esp32-01", false).unwrap();
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM readings WHERE device_id='esp32-01'", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 1);
    }
}
