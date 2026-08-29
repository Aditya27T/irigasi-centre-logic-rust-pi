mod water_balance;

use std::collections::HashMap;

use crate::model::sensor::SensorReading;

#[derive(Debug, Clone, Default)]
pub struct ZoneTwinState {
    pub last_soil_moisture_1_pct: Option<f32>,
    pub last_soil_moisture_2_pct: Option<f32>,
    pub last_temp_1_c: Option<f32>,
    pub last_temp_2_c: Option<f32>,
    pub valve_1_open: bool,
    pub valve_2_open: bool,
    /// Placeholder, belum dipakai decision engine di MVP -- lihat plan.md §4/§8.
    pub water_balance_mm: f32,
    pub last_reading_at: Option<i64>,
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

    pub fn set_valve_state(&mut self, device_id: &str, valve_id: u8, open: bool) {
        let state = self.zones.entry(device_id.to_string()).or_default();
        match valve_id {
            1 => state.valve_1_open = open,
            2 => state.valve_2_open = open,
            _ => {}
        }
    }
}
