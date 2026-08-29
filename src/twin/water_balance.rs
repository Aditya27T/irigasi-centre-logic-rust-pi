//! Accumulator water-balance -- placeholder MVP, dicatat tapi belum dipakai
//! decision engine (plan.md §4). Seam terbuka untuk Fase D.

use super::ZoneTwinState;
use crate::model::sensor::SensorReading;

pub fn accumulate(state: &mut ZoneTwinState, reading: &SensorReading) {
    let avg_soil = (reading.soil_moisture_1_pct + reading.soil_moisture_2_pct) / 2.0;
    let deficit = (50.0 - avg_soil).max(0.0) * 0.1;
    state.water_balance_mm += deficit;
}
