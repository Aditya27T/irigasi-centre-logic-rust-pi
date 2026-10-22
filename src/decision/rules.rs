use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

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
    /// Terisi hanya kalau action=Open dipicu `scheduled_window`. Dipakai pemanggil untuk
    /// menandai window ini sudah dijalankan (lihat `Twin::set_schedule_window_ran`) setelah
    /// command berhasil dipublish, supaya valve tidak berdenyut berulang sepanjang window.
    pub schedule_window_key: Option<String>,
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
            schedule_window_key: None,
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
            schedule_window_key: None,
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
            schedule_window_key: None,
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
            schedule_window_key: None,
        }
    }
}

/// `now` memakai NaiveDateTime (bukan hanya NaiveTime) karena penanda "window sudah
/// dijalankan" butuh tanggal: tanpa itu, window 06:00 hari ini akan memblokir 06:00 besok.
pub fn evaluate(
    reading: &SensorReading,
    node_cfg: &NodeConfig,
    twin: &ZoneTwinState,
    now: NaiveDateTime,
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
            twin.valve_1_last_window.as_deref(),
        ),
        evaluate_valve_guarded(
            2,
            reading.soil_moisture_2_pct,
            reading.temp_2_c,
            &node_cfg.valve_2,
            twin.valve_2_open,
            &twin.valve_2_source,
            now,
            twin.valve_2_last_window.as_deref(),
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn evaluate_valve_guarded(
    valve_id: u8,
    soil_pct: f32,
    temp_c: f32,
    cfg: &ValveConfig,
    is_open: bool,
    source: &str,
    now: NaiveDateTime,
    last_window: Option<&str>,
) -> DecisionOutcome {
    if source == "manual" {
        return DecisionOutcome::manual_hold(valve_id, soil_pct, temp_c);
    }
    evaluate_valve(valve_id, soil_pct, temp_c, cfg, is_open, now, last_window)
}

fn evaluate_valve(
    valve_id: u8,
    soil_pct: f32,
    temp_c: f32,
    cfg: &ValveConfig,
    is_open: bool,
    now: NaiveDateTime,
    last_window: Option<&str>,
) -> DecisionOutcome {
    let (close_temp, close_soil, max_duration) = cfg.close_params();
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
            // Buka SEKALI per window. `is_open` saja tidak cukup: nilainya mengikuti ground
            // truth relay ESP32, yang menutup sendiri setelah max_open_duration_s -- tanpa
            // penanda window, pembacaan berikutnya akan membuka ulang dan valve berdenyut
            // berkali-kali sepanjang window (mis. ~7x untuk window 15 menit & durasi 120s).
            if let Some(win) = current_window(now.time(), schedule) {
                let key = window_key(now.date(), win);
                if !is_open && last_window != Some(key.as_str()) {
                    let mut outcome = DecisionOutcome::open(
                        valve_id,
                        max_duration,
                        soil_pct,
                        temp_c,
                        "scheduled_window",
                        None,
                    );
                    outcome.schedule_window_key = Some(key);
                    return outcome;
                }
            }
        }
    }
    DecisionOutcome::none(valve_id, soil_pct, temp_c)
}

/// Window yang sedang berjalan sekarang, kalau ada.
fn current_window(now: NaiveTime, windows: &[ScheduleWindow]) -> Option<&ScheduleWindow> {
    windows.iter().find(|w| {
        match (crate::config::parse_hhmm(&w.start), crate::config::parse_hhmm(&w.end)) {
            (Some(start), Some(end)) => now >= start && now < end,
            _ => false,
        }
    })
}

/// Identitas satu kemunculan window: tanggal + jam mulai, mis. "2026-09-13 06:00".
/// Tanggal wajib ikut supaya window yang sama besok dianggap kemunculan baru.
fn window_key(today: NaiveDate, win: &ScheduleWindow) -> String {
    format!("{} {}", today, win.start)
}

pub fn is_within_schedule(now: NaiveTime, windows: &[ScheduleWindow]) -> bool {
    windows.iter().any(|w| {
        match (crate::config::parse_hhmm(&w.start), crate::config::parse_hhmm(&w.end)) {
            (Some(start), Some(end)) => now >= start && now < end,
            _ => false,
        }
    })
}

pub fn active_or_next_window(now: NaiveTime, windows: &[ScheduleWindow]) -> Option<(&ScheduleWindow, bool)> {
    let mut parsed: Vec<(NaiveTime, NaiveTime, &ScheduleWindow)> = Vec::new();
    for w in windows {
        if let (Some(s), Some(e)) = (crate::config::parse_hhmm(&w.start), crate::config::parse_hhmm(&w.end)) {
            if s < e {
                parsed.push((s, e, w));
            }
        }
    }
    parsed.sort_by_key(|(s, _, _)| *s);
    for (s, e, w) in &parsed {
        if now >= *s && now < *e {
            return Some((*w, true));
        }
    }
    for (s, _, w) in &parsed {
        if now < *s {
            return Some((*w, false));
        }
    }
    if let Some((_, _, w)) = parsed.first() {
        return Some((*w, false));
    }
    None
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

    /// Tanggal dipatok supaya window_key deterministik dan test tidak bergantung hari ini.
    fn dt(hhmm: &str) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 13).unwrap().and_time(t(hhmm))
    }

    #[test]
    fn sensor_mode_opens_when_hot_and_dry() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve(1, 30.0, 28.0, &cfg, false, dt("10:00"), None);
        assert_eq!(outcome.action, Action::Open);
    }

    #[test]
    fn sensor_mode_closes_when_cool_and_wet() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve(1, 50.0, 20.0, &cfg, true, dt("10:00"), None);
        assert_eq!(outcome.action, Action::Close);
    }

    #[test]
    fn sensor_mode_none_in_hysteresis_gap() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve(1, 40.0, 26.0, &cfg, false, dt("10:00"), None);
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn schedule_mode_opens_within_window_when_not_already_open() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, dt("06:05"), None);
        assert_eq!(outcome.action, Action::Open);
    }

    #[test]
    fn schedule_mode_does_not_reopen_if_already_open() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, true, dt("06:05"), None);
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn schedule_mode_close_condition_overrides_schedule() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        let outcome = evaluate_valve(2, 50.0, 20.0, &cfg, true, dt("06:05"), None);
        assert_eq!(outcome.action, Action::Close);
    }

    #[test]
    fn schedule_mode_none_outside_window() {
        let cfg = schedule_cfg(vec![ScheduleWindow {
            start: "06:00".into(),
            end: "06:15".into(),
        }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, dt("12:00"), None);
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn manual_source_holds_decision_engine_even_when_open_condition_met() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve_guarded(1, 30.0, 28.0, &cfg, false, "manual", dt("10:00"), None);
        assert_eq!(outcome.action, Action::None);
        assert_eq!(outcome.reason, "manual_hold");
    }

    #[test]
    fn schedule_open_mengisi_schedule_window_key_dengan_tanggal() {
        let cfg = schedule_cfg(vec![ScheduleWindow { start: "06:00".into(), end: "06:15".into() }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, dt("06:05"), None);
        assert_eq!(outcome.action, Action::Open);
        // Tanggal wajib ikut, kalau tidak window 06:00 besok akan ikut terblokir.
        assert_eq!(outcome.schedule_window_key.as_deref(), Some("2026-09-13 06:00"));
    }

    #[test]
    fn schedule_mode_tidak_buka_ulang_di_window_yang_sama() {
        let cfg = schedule_cfg(vec![ScheduleWindow { start: "06:00".into(), end: "06:15".into() }]);
        // is_open=false meniru kondisi setelah ESP32 menutup relay sendiri di tengah window;
        // sebelum perbaikan ini kondisi itu memicu open berulang tiap pembacaan (~2 detik).
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, dt("06:10"), Some("2026-09-13 06:00"));
        assert_eq!(outcome.action, Action::None);
    }

    #[test]
    fn schedule_mode_buka_lagi_di_window_kedua_hari_yang_sama() {
        let cfg = schedule_cfg(vec![
            ScheduleWindow { start: "06:00".into(), end: "06:15".into() },
            ScheduleWindow { start: "17:00".into(), end: "17:15".into() },
        ]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, dt("17:05"), Some("2026-09-13 06:00"));
        assert_eq!(outcome.action, Action::Open);
        assert_eq!(outcome.schedule_window_key.as_deref(), Some("2026-09-13 17:00"));
    }

    #[test]
    fn schedule_mode_buka_lagi_saat_window_sama_berulang_besok() {
        let cfg = schedule_cfg(vec![ScheduleWindow { start: "06:00".into(), end: "06:15".into() }]);
        let outcome = evaluate_valve(2, 30.0, 20.0, &cfg, false, dt("06:05"), Some("2026-09-12 06:00"));
        assert_eq!(outcome.action, Action::Open);
    }

    #[test]
    fn auto_source_evaluates_normally() {
        let cfg = sensor_cfg();
        let outcome = evaluate_valve_guarded(1, 30.0, 28.0, &cfg, false, "auto", dt("10:00"), None);
        assert_eq!(outcome.action, Action::Open);
    }

    #[test]
    fn active_or_next_window_mengembalikan_window_yang_sedang_jalan() {
        let windows = vec![
            ScheduleWindow { start: "06:00".into(), end: "06:15".into() },
            ScheduleWindow { start: "17:00".into(), end: "17:15".into() },
        ];
        let (w, running) = active_or_next_window(t("06:05"), &windows).unwrap();
        assert_eq!(w.start, "06:00");
        assert!(running);
    }

    #[test]
    fn active_or_next_window_mengembalikan_window_berikutnya_hari_ini() {
        let windows = vec![
            ScheduleWindow { start: "06:00".into(), end: "06:15".into() },
            ScheduleWindow { start: "17:00".into(), end: "17:15".into() },
        ];
        let (w, running) = active_or_next_window(t("10:00"), &windows).unwrap();
        assert_eq!(w.start, "17:00");
        assert!(!running);
    }

    #[test]
    fn active_or_next_window_wrap_ke_window_pertama_besok() {
        let windows = vec![
            ScheduleWindow { start: "06:00".into(), end: "06:15".into() },
            ScheduleWindow { start: "17:00".into(), end: "17:15".into() },
        ];
        let (w, running) = active_or_next_window(t("20:00"), &windows).unwrap();
        assert_eq!(w.start, "06:00");
        assert!(!running);
    }

    #[test]
    fn active_or_next_window_konsisten_dengan_is_within_schedule() {
        let windows = vec![
            ScheduleWindow { start: "06:00".into(), end: "06:15".into() },
            ScheduleWindow { start: "17:00".into(), end: "17:15".into() },
        ];
        for h in 0..24 {
            for m in 0..60 {
                let now = NaiveTime::from_hms_opt(h, m, 0).unwrap();
                let within = is_within_schedule(now, &windows);
                let active = active_or_next_window(now, &windows);
                let running = active.map(|(_, r)| r).unwrap_or(false);
                assert_eq!(within, running, "mismatch at {now}");
            }
        }
    }
}
