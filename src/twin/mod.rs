mod water_balance;

use std::collections::HashMap;

use crate::model::sensor::SensorReading;

#[derive(Debug, Clone)]
pub struct ZoneTwinState {
    pub last_soil_moisture_1_pct: Option<f32>,
    pub last_soil_moisture_2_pct: Option<f32>,
    pub last_temp_1_c: Option<f32>,
    pub last_temp_2_c: Option<f32>,
    pub valve_1_open: bool,
    pub valve_2_open: bool,
    /// "auto" (decision engine) atau "manual" (override dashboard).
    pub valve_1_source: String,
    /// Epoch detik saat valve_1 dijadwalkan otomatis tertutup lagi (None kalau tertutup).
    pub valve_1_expires_at: Option<i64>,
    pub valve_2_source: String,
    pub valve_2_expires_at: Option<i64>,
    /// Placeholder, belum dipakai decision engine di MVP -- lihat plan.md §4/§8.
    pub water_balance_mm: f32,
    pub last_reading_at: Option<i64>,
}

impl Default for ZoneTwinState {
    fn default() -> Self {
        Self {
            last_soil_moisture_1_pct: None,
            last_soil_moisture_2_pct: None,
            last_temp_1_c: None,
            last_temp_2_c: None,
            valve_1_open: false,
            valve_2_open: false,
            valve_1_source: "auto".to_string(),
            valve_1_expires_at: None,
            valve_2_source: "auto".to_string(),
            valve_2_expires_at: None,
            water_balance_mm: 0.0,
            last_reading_at: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Twin {
    zones: HashMap<String, ZoneTwinState>,
}

impl Twin {
    pub fn update_from_reading(&mut self, reading: &SensorReading, received_at: i64) {
        let state = self.zones.entry(reading.device_id.clone()).or_default();
        state.last_soil_moisture_1_pct = Some(reading.soil_moisture_1_pct);
        state.last_soil_moisture_2_pct = Some(reading.soil_moisture_2_pct);
        state.last_temp_1_c = Some(reading.temp_1_c);
        state.last_temp_2_c = Some(reading.temp_2_c);
        state.last_reading_at = Some(received_at);
        water_balance::accumulate(state, reading);
    }

    pub fn get(&self, device_id: &str) -> Option<&ZoneTwinState> {
        self.zones.get(device_id)
    }

    pub fn set_valve_state(
        &mut self,
        device_id: &str,
        valve_id: u8,
        open: bool,
        source: &str,
        expires_at: Option<i64>,
    ) {
        let state = self.zones.entry(device_id.to_string()).or_default();
        match valve_id {
            1 => {
                state.valve_1_open = open;
                state.valve_1_source = source.to_string();
                state.valve_1_expires_at = expires_at;
            }
            2 => {
                state.valve_2_open = open;
                state.valve_2_source = source.to_string();
                state.valve_2_expires_at = expires_at;
            }
            _ => {}
        }
    }
}
