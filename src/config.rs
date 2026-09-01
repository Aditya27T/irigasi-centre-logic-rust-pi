use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub mqtt: MqttConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub decision: DecisionConfig,
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
    /// Label zona/plot yang mudah dibaca manusia (mis. "Zona A"), ditampilkan di dashboard.
    #[serde(default)]
    pub zone: Option<String>,
    pub valve_1: ValveConfig,
    pub valve_2: ValveConfig,
}

/// Konfigurasi 1 valve. Mode "sensor" = threshold suhu+kelembapan (hysteresis,
/// mereplikasi logika asli firmware). Mode "schedule" = buka per jam, tapi
/// kondisi close tetap sama di kedua mode. Lihat plan.md §4.
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
    /// (close_temp_threshold_c, close_soil_min_pct, max_open_duration_s) -- sama di kedua mode.
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleWindow {
    /// Format "HH:MM", waktu lokal Pi.
    pub start: String,
    pub end: String,
}

/// Konfigurasi dashboard HTTP (feature "dashboard"). Field ini tetap ada tanpa
/// cfg-gate supaya config.rs kompilasi sama persis dengan/tanpa feature, dan
/// config lama tanpa [dashboard] tetap valid (default: nonaktif).
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

impl Config {
    pub fn load(path: &str) -> Result<Config> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("membaca file config {path}"))?;
        let config: Config =
            toml::from_str(&raw).with_context(|| format!("parsing TOML config {path}"))?;
        Ok(config)
    }
}
