use anyhow::{Context, Result};
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

pub type SharedNodes = Arc<RwLock<Vec<NodeConfig>>>;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub decision: DecisionConfig,
    #[serde(default)]
    pub nodes: Vec<NodeConfig>,
    #[serde(default)]
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Deserialize)]
pub struct MqttConfig {
    pub broker_host: String,
    pub broker_port: u16,
    #[serde(default = "default_client_id")]
    pub client_id: String,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

fn default_client_id() -> String {
    "irigasi-core".to_string()
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub sqlite_path: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct DecisionConfig {
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeConfig {
    pub id: String,
    #[serde(default)]
    pub zone: Option<String>,
    pub valve_1: ValveConfig,
    pub valve_2: ValveConfig,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ValveConfig {
    Sensor {
        open_temp_threshold_c: f32,
        open_soil_max_pct: f32,
        close_temp_threshold_c: f32,
        close_soil_min_pct: f32,
        max_open_duration_s: u32,
    },
    Schedule {
        close_temp_threshold_c: f32,
        close_soil_min_pct: f32,
        max_open_duration_s: u32,
        schedule: Vec<ScheduleWindow>,
    },
}

impl ValveConfig {
    pub fn close_params(&self) -> (f32, f32, u32) {
        match self {
            ValveConfig::Sensor {
                close_temp_threshold_c,
                close_soil_min_pct,
                max_open_duration_s,
                ..
            }
            | ValveConfig::Schedule {
                close_temp_threshold_c,
                close_soil_min_pct,
                max_open_duration_s,
                ..
            } => (*close_temp_threshold_c, *close_soil_min_pct, *max_open_duration_s),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let (close_temp, close_soil, max_dur) = self.close_params();
        if !close_temp.is_finite() || !close_soil.is_finite() {
            return Err("threshold harus angka finite".to_string());
        }
        if !(0.0..=100.0).contains(&close_soil) {
            return Err("close_soil_min_pct harus 0..100".to_string());
        }
        if !(-10.0..=80.0).contains(&close_temp) {
            return Err("close_temp_threshold_c harus -10..80".to_string());
        }
        if !(1..=3600).contains(&max_dur) {
            return Err("max_open_duration_s harus 1..3600".to_string());
        }
        match self {
            ValveConfig::Sensor {
                open_temp_threshold_c,
                open_soil_max_pct,
                ..
            } => {
                if !open_temp_threshold_c.is_finite() || !open_soil_max_pct.is_finite() {
                    return Err("threshold harus angka finite".to_string());
                }
                if !(0.0..=100.0).contains(open_soil_max_pct) {
                    return Err("open_soil_max_pct harus 0..100".to_string());
                }
                if !(-10.0..=80.0).contains(open_temp_threshold_c) {
                    return Err("open_temp_threshold_c harus -10..80".to_string());
                }
                if *open_temp_threshold_c < close_temp {
                    return Err("open_temp_threshold_c harus >= close_temp_threshold_c".to_string());
                }
                if *open_soil_max_pct > close_soil {
                    return Err("open_soil_max_pct harus <= close_soil_min_pct".to_string());
                }
            }
            ValveConfig::Schedule { schedule, .. } => {
                validate_schedule_windows(schedule)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleWindow {
    pub start: String,
    pub end: String,
}

fn validate_schedule_windows(windows: &[ScheduleWindow]) -> Result<(), String> {
    if windows.len() > 12 {
        return Err("maksimal 12 window per valve".to_string());
    }
    let mut parsed: Vec<(NaiveTime, NaiveTime, usize)> = Vec::new();
    for (idx, w) in windows.iter().enumerate() {
        let start = parse_hhmm(&w.start).ok_or_else(|| format!("window {} start '{}' tidak valid (HH:MM)", idx + 1, w.start))?;
        let end = parse_hhmm(&w.end).ok_or_else(|| format!("window {} end '{}' tidak valid (HH:MM)", idx + 1, w.end))?;
        if start >= end {
            if start > end {
                return Err(format!("window {} {}-{} melewati tengah malam; pecah jadi {}-23:59 dan 00:00-{}", idx + 1, w.start, w.end, w.start, w.end));
            } else {
                return Err(format!("window {} start harus < end", idx + 1));
            }
        }
        parsed.push((start, end, idx));
    }
    parsed.sort_by_key(|(s, _, _)| *s);
    for i in 1..parsed.len() {
        let (prev_s, prev_e, prev_idx) = parsed[i - 1];
        let (cur_s, cur_e, cur_idx) = parsed[i];
        if prev_s == cur_s && prev_e == cur_e {
            return Err(format!("window {} duplikat", cur_idx + 1));
        }
        if cur_s < prev_e {
            return Err(format!("window tumpang tindih: {}-{} dan {}-{}", format_hhmm(prev_s), format_hhmm(prev_e), format_hhmm(cur_s), format_hhmm(cur_e)));
        }
        let _ = prev_idx;
    }
    Ok(())
}

fn format_hhmm(t: NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

pub(crate) fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    if s.len() != 5 || s.as_bytes()[2] != b':' {
        return None;
    }
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

impl NodeConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("id tidak boleh kosong".to_string());
        }
        if self.id.len() > 64 {
            return Err("id maksimal 64 karakter".to_string());
        }
        if !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err("id hanya boleh A-Z a-z 0-9 _ -".to_string());
        }
        if let Some(z) = &self.zone {
            let trimmed = z.trim();
            if trimmed.len() > 64 {
                return Err("zone maksimal 64 karakter".to_string());
            }
        }
        self.valve_1.validate().map_err(|e| format!("valve_1 tidak valid: {e}"))?;
        self.valve_2.validate().map_err(|e| format!("valve_2 tidak valid: {e}"))?;
        Ok(())
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for n in &self.nodes {
            n.validate()?;
        }
        let mut seen = std::collections::HashSet::new();
        for n in &self.nodes {
            if !seen.insert(&n.id) {
                return Err(format!("id duplikat: {}", n.id));
            }
        }
        Ok(())
    }

    pub fn load(path: &str) -> Result<Config> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("membaca file config {path}"))?;
        let mut config: Config =
            toml::from_str(&raw).with_context(|| format!("parsing TOML config {path}"))?;
        for node in &mut config.nodes {
            if let Some(z) = node.zone.take() {
                let trimmed = z.trim().to_string();
                if trimmed.is_empty() {
                    node.zone = None;
                } else {
                    node.zone = Some(trimmed);
                }
            }
        }
        config.validate().map_err(|e| anyhow::anyhow!(e))?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
pub struct DashboardConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: default_bind_addr(),
        }
    }
}

fn default_bind_addr() -> String {
    "0.0.0.0:8080".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule_cfg(windows: Vec<(&str, &str)>) -> ValveConfig {
        ValveConfig::Schedule {
            close_temp_threshold_c: 25.0,
            close_soil_min_pct: 45.0,
            max_open_duration_s: 120,
            schedule: windows.into_iter().map(|(s, e)| ScheduleWindow { start: s.to_string(), end: e.to_string() }).collect(),
        }
    }

    #[test]
    fn hhmm_valid_dan_invalid() {
        assert!(parse_hhmm("06:00").is_some());
        assert!(parse_hhmm("23:59").is_some());
        assert!(parse_hhmm("24:00").is_none());
        assert!(parse_hhmm("6:00").is_none());
        assert!(parse_hhmm("").is_none());
    }

    #[test]
    fn window_terbalik_ditolak() {
        let cfg = schedule_cfg(vec![("10:00", "09:00")]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn window_lewat_tengah_malam_ditolak() {
        let cfg = schedule_cfg(vec![("22:00", "02:00")]);
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("tengah malam"), "pesan harus menyebut tengah malam, got: {err}");
    }

    #[test]
    fn window_tumpang_tindih_ditolak() {
        let cfg = schedule_cfg(vec![("06:00", "06:15"), ("06:10", "06:20")]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn jadwal_kosong_diterima() {
        let cfg = schedule_cfg(vec![]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn sensor_mode_butuh_hysteresis_konsisten() {
        let cfg = ValveConfig::Sensor {
            open_temp_threshold_c: 20.0,
            open_soil_max_pct: 50.0,
            close_temp_threshold_c: 25.0,
            close_soil_min_pct: 45.0,
            max_open_duration_s: 120,
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.contains("open_temp") || err.contains("open_soil"), "{err}");
    }

    #[test]
    fn device_id_dengan_karakter_topic_mqtt_ditolak() {
        let n = NodeConfig {
            id: "esp32/+".to_string(),
            zone: None,
            valve_1: schedule_cfg(vec![]),
            valve_2: schedule_cfg(vec![]),
            enabled: true,
        };
        assert!(n.validate().is_err());
    }

    #[test]
    fn valve_config_round_trip_json() {
        let cfg = schedule_cfg(vec![("06:00", "06:15")]);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"mode\":\"schedule\""));
        let parsed: ValveConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.validate().is_ok());
    }
}
