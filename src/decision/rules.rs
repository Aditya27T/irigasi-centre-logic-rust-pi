use chrono::NaiveTime;

use crate::config::{NodeConfig, ScheduleWindow, ValveConfig};
use crate::model::sensor::SensorReading;
use crate::twin::ZoneTwinState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Close,
    None,
}

#[derive(Debug, Clone)]
pub struct DecisionOutcome {
    pub valve_id: u8,
    pub action: Action,
    pub duration_s: Option<u32>,
    pub reason: &'static str,
    pub trigger_soil_moisture_pct: f32,
    pub trigger_temp_c: f32,
    pub threshold_used: Option<f32>,
}

impl DecisionOutcome {
    fn open(valve_id: u8, duration_s: u32, soil_pct: f32, temp_c: f32, reason: &'static str, threshold: Option<f32>) -> Self {
        Self {
            valve_id,
            action: Action::Open,
            duration_s: Some(duration_s),
            reason,
            trigger_soil_moisture_pct: soil_pct,
            trigger_temp_c: temp_c,
            threshold_used: threshold,
        }
    }

    fn close(valve_id: u8, soil_pct: f32, temp_c: f32, threshold: f32) -> Self {
        Self {
            valve_id,
            action: Action::Close,
            duration_s: None,
            reason: "close_condition_met",
            trigger_soil_moisture_pct: soil_pct,
            trigger_temp_c: temp_c,
            threshold_used: Some(threshold),
        }
    }

    fn none(valve_id: u8, soil_pct: f32, temp_c: f32) -> Self {
        Self {
            valve_id,
            action: Action::None,
            duration_s: None,
            reason: "no_trigger",
            trigger_soil_moisture_pct: soil_pct,
            trigger_temp_c: temp_c,
            threshold_used: None,
        }
    }

    fn manual_hold(valve_id: u8, soil_pct: f32, temp_c: f32) -> Self {
        Self {
            valve_id,
            action: Action::None,
            duration_s: None,
            reason: "manual_hold",
            trigger_soil_moisture_pct: soil_pct,
            trigger_temp_c: temp_c,
            threshold_used: None,
        }
    }
}

/// Evaluasi 2 valve independen per node -- valve 1 dari pasangan sensor 1,
/// valve 2 dari pasangan sensor 2. Lihat plan.md §4.
pub fn evaluate(
    reading: &SensorReading,
    node_cfg: &NodeConfig,
    twin: &ZoneTwinState,
    now: NaiveTime,
) -> Vec<DecisionOutcome> {
    vec![
        evaluate_valve_guarded(
            1,
            reading.soil_moisture_1_pct,
            reading.temp_1_c,
            &node_cfg.valve_1,
            twin.valve_1_open,
            &twin.valve_1_source,
            now,
        ),
        evaluate_valve_guarded(
            2,
            reading.soil_moisture_2_pct,
            reading.temp_2_c,
            &node_cfg.valve_2,
            twin.valve_2_open,
            &twin.valve_2_source,
            now,
        ),
    ]
}

/// Valve yang sedang di-override manual dari dashboard tidak dievaluasi decision
/// engine sampai dilepas lagi ke "auto" lewat tombol "Lepas ke Otomatis".
fn evaluate_valve_guarded(
    valve_id: u8,
    soil_pct: f32,
    temp_c: f32,
    cfg: &ValveConfig,
    is_open: bool,
    source: &str,
    now: NaiveTime,
) -> DecisionOutcome {
    if source == "manual" {
        return DecisionOutcome::manual_hold(valve_id, soil_pct, temp_c);
    }
    evaluate_valve(valve_id, soil_pct, temp_c, cfg, is_open, now)
}

fn evaluate_valve(
    valve_id: u8,
    soil_pct: f32,
    temp_c: f32,
    cfg: &ValveConfig,
    is_open: bool,
    now: NaiveTime,
) -> DecisionOutcome {
    let (close_temp, close_soil, max_duration) = cfg.close_params();

    // Kondisi close berlaku sama di kedua mode (sensor & schedule) -- keputusan tim.
    if temp_c < close_temp && soil_pct >= close_soil {
        return DecisionOutcome::close(valve_id, soil_pct, temp_c, close_soil);
    }

    match cfg {
        ValveConfig::Sensor {
            open_temp_threshold_c,
            open_soil_max_pct,
            ..
        } => {
            if temp_c >= *open_temp_threshold_c && soil_pct <= *open_soil_max_pct {
                return DecisionOutcome::open(
                    valve_id,
                    max_duration,
                    soil_pct,
                    temp_c,
                    "soil_moisture_below_threshold",
                    Some(*open_soil_max_pct),
                );
            }
        }
        ValveConfig::Schedule { schedule, .. } => {
            if !is_open && is_within_schedule(now, schedule) {
                return DecisionOutcome::open(
                    valve_id,
                    max_duration,
                    soil_pct,
                    temp_c,
                    "scheduled_window",
                    None,
                );
            }
        }
    }

    DecisionOutcome::none(valve_id, soil_pct, temp_c)
}

fn is_within_schedule(now: NaiveTime, windows: &[ScheduleWindow]) -> bool {
    windows.iter().any(|w| {
        match (parse_hhmm(&w.start), parse_hhmm(&w.end)) {
            (Some(start), Some(end)) => now >= start && now < end,
            _ => false,
        }
    })
}

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor_cfg() -> ValveConfig {
        ValveConfig::Sensor {
            open_temp_threshold_c: 27.0,
            open_soil_max_pct: 35.0,
            close_temp_threshold_c: 25.0,
            close_soil_min_pct: 45.0,
            max_open_duration_s: 120,
        }
    }

    fn schedule_cfg(windows: Vec<ScheduleWindow>) -> ValveConfig {
        ValveConfig::Schedule {
            close_temp_threshold_c: 25.0,
            close_soil_min_pct: 45.0,
            max_open_duration_s: 900,
            schedule: windows,
        }
    }

    fn t(hhmm: &str) -> NaiveTime {
        NaiveTime::parse_from_str(hhmm, "%H:%M").unwrap()
    }

    #[test]
    fn sensor_mode_opens_when_hot_and_dry() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve(1, 30.0, 28.0, &cfg, false, t("10:00"));
        assert_eq!(outcome.action, Action::Open);
    }

    #[test]
    fn sensor_mode_closes_when_cool_and_wet() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve(1, 50.0, 20.0, &cfg, true, t("10:00"));
        assert_eq!(outcome.action, Action::Close);
    }

    #[test]
    fn sensor_mode_none_in_hysteresis_gap() {
        let cfg = sensor_cfg();
        // suhu & kelembapan di antara open- dan close-threshold -> None (state dipertahankan).
        let outcome = evaluate_valve(1, 40.0, 26.0, &cfg, false, t("10:00"));
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn schedule_mode_opens_within_window_when_not_already_open() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        // soil/temp sengaja dipilih supaya TIDAK memicu kondisi close (30 < close_soil_min 45).
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, t("06:05"));
        assert_eq!(outcome.action, Action::Open);
    }

    #[test]
    fn schedule_mode_does_not_reopen_if_already_open() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, true, t("06:05"));
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn schedule_mode_close_condition_overrides_schedule() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        // Dalam window jadwal, tapi tanah sudah basah & sejuk -> tetap close.
        let outcome = evaluate_valve(2, 50.0, 20.0, &cfg, true, t("06:05"));
        assert_eq!(outcome.action, Action::Close);
    }

    #[test]
    fn schedule_mode_none_outside_window() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, t("12:00"));
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn manual_source_holds_decision_engine_even_when_open_condition_met() {
        let cfg = sensor_cfg();
        // Kondisi ini biasanya memicu Open (panas & kering), tapi source "manual"
        // harus menahan engine sampai dilepas lewat tombol "Lepas ke Otomatis".
        let outcome = evaluate_valve_guarded(1, 30.0, 28.0, &cfg, false, "manual", t("10:00"));
        assert_eq!(outcome.action, Action::None);
        assert_eq!(outcome.reason, "manual_hold");
    }

    #[test]
    fn auto_source_evaluates_normally() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve_guarded(1, 30.0, 28.0, &cfg, false, "auto", t("10:00"));
        assert_eq!(outcome.action, Action::Open);
    }
}
