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
    pub valve_1_source: String,
    pub valve_1_expires_at: Option<i64>,
    pub valve_2_source: String,
    pub valve_2_expires_at: Option<i64>,
    pub last_reading_at: Option<i64>,
    /// Penanda "jendela jadwal ini sudah dijalankan", format "YYYY-MM-DD HH:MM" dari awal
    /// window. Tanpa ini valve berdenyut berulang sepanjang window: ESP32 menutup relay
    /// sendiri setelah max_open_duration_s, `valve_N_open` ikut jadi false dari ground truth
    /// relay, lalu pembacaan berikutnya (~2 detik) memicu `scheduled_window` lagi.
    ///
    /// Sengaja HANYA di memori, tidak dipersist ke twin_state: nilainya cuma berguna selama
    /// satu window berjalan, dan kolom DB yang ditulis tapi tak pernah dibaca balik justru
    /// jadi beban mati (persis kesalahan `water_balance_mm` yang baru dihapus). Konsekuensi
    /// yang diterima: restart di tengah window bisa menghasilkan maksimal satu denyut ekstra.
    pub valve_1_last_window: Option<String>,
    pub valve_2_last_window: Option<String>,
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
            last_reading_at: None,
            valve_1_last_window: None,
            valve_2_last_window: None,
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
        if let Some(open) = reading.relay_1_open() {
            state.valve_1_open = open;
        }
        if let Some(open) = reading.relay_2_open() {
            state.valve_2_open = open;
        }
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

    /// Catat bahwa window jadwal `key` sudah dijalankan untuk valve ini. Dipanggil hanya
    /// setelah command open berhasil dipublish -- kalau publish gagal, window belum benar-benar
    /// dijalankan dan harus boleh dicoba lagi di pembacaan berikutnya.
    pub fn set_schedule_window_ran(&mut self, device_id: &str, valve_id: u8, key: String) {
        let state = self.zones.entry(device_id.to_string()).or_default();
        match valve_id {
            1 => state.valve_1_last_window = Some(key),
            2 => state.valve_2_last_window = Some(key),
            _ => {}
        }
    }

    /// Lepas manual override yang sudah melewati `expires_at` kembali ke otomatis.
    /// Mengembalikan valve_id yang dilepas (untuk logging). Override tanpa `expires_at`
    /// (mis. manual close tanpa durasi) sengaja ditahan selamanya sampai dilepas eksplisit
    /// lewat POST .../release -- itu memang maksud operator saat tidak memberi durasi.
    pub fn release_expired_manual(&mut self, device_id: &str, now_epoch: i64) -> Vec<u8> {
        let Some(state) = self.zones.get_mut(device_id) else {
            return Vec::new();
        };
        let mut released = Vec::new();
        if state.valve_1_source == "manual"
            && state.valve_1_expires_at.is_some_and(|e| now_epoch >= e)
        {
            state.valve_1_source = "auto".to_string();
            state.valve_1_expires_at = None;
            // Lihat komentar di release_to_auto: kembali ke auto berarti mesin schedule
            // boleh mempertimbangkan lagi dari awal, termasuk membuka ulang kalau window
            // masih berjalan.
            state.valve_1_last_window = None;
            released.push(1);
        }
        if state.valve_2_source == "manual"
            && state.valve_2_expires_at.is_some_and(|e| now_epoch >= e)
        {
            state.valve_2_source = "auto".to_string();
            state.valve_2_expires_at = None;
            state.valve_2_last_window = None;
            released.push(2);
        }
        released
    }

    pub fn release_to_auto(&mut self, device_id: &str, valve_id: u8) {
        let state = self.zones.entry(device_id.to_string()).or_default();
        match valve_id {
            1 => {
                state.valve_1_source = "auto".to_string();
                state.valve_1_expires_at = None;
                // BUG (dilaporkan pengguna): tanpa baris ini, valve yang dimatikan manual lalu
                // dikembalikan ke otomatis DI DALAM window jadwal yang sudah pernah berjalan
                // tetap diam di OFF sampai window berikutnya -- penanda "window ini sudah
                // dijalankan" (dipakai untuk mencegah denyut berulang, lihat komentar di
                // struct) masih menunjuk window yang sama, jadi evaluate_valve menolak
                // membukanya lagi. "Kembali ke otomatis" seharusnya berarti mesin schedule
                // memutuskan dari awal lagi, termasuk membuka ulang kalau window masih aktif.
                state.valve_1_last_window = None;
            }
            2 => {
                state.valve_2_source = "auto".to_string();
                state.valve_2_expires_at = None;
                state.valve_2_last_window = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twin_with_manual(valve_id: u8, expires_at: Option<i64>) -> Twin {
        let mut twin = Twin::default();
        twin.set_valve_state("esp32-01", valve_id, true, "manual", expires_at);
        twin
    }

    #[test]
    fn manual_override_yang_lewat_expires_at_dilepas_ke_auto() {
        let mut twin = twin_with_manual(1, Some(1_000));
        assert_eq!(twin.release_expired_manual("esp32-01", 1_001), vec![1]);
        let state = twin.get("esp32-01").unwrap();
        assert_eq!(state.valve_1_source, "auto");
        assert_eq!(state.valve_1_expires_at, None);
    }

    #[test]
    fn manual_override_yang_belum_lewat_tetap_ditahan() {
        let mut twin = twin_with_manual(1, Some(1_000));
        assert!(twin.release_expired_manual("esp32-01", 999).is_empty());
        assert_eq!(twin.get("esp32-01").unwrap().valve_1_source, "manual");
    }

    #[test]
    fn manual_override_tanpa_expires_at_ditahan_selamanya() {
        // Manual close tanpa durasi memang dimaksudkan menahan sampai dilepas eksplisit
        // lewat POST .../release, jadi jangan ikut dilepas oleh kedaluwarsa waktu.
        let mut twin = twin_with_manual(2, None);
        assert!(twin.release_expired_manual("esp32-01", i64::MAX).is_empty());
        assert_eq!(twin.get("esp32-01").unwrap().valve_2_source, "manual");
    }

    #[test]
    fn release_expired_manual_aman_untuk_device_yang_belum_dikenal() {
        let mut twin = Twin::default();
        assert!(twin.release_expired_manual("belum-ada", 123).is_empty());
    }

    #[test]
    fn set_schedule_window_ran_menandai_valve_yang_benar() {
        let mut twin = Twin::default();
        twin.set_schedule_window_ran("esp32-01", 2, "2026-09-13 06:00".to_string());
        let state = twin.get("esp32-01").unwrap();
        assert_eq!(state.valve_1_last_window, None);
        assert_eq!(state.valve_2_last_window.as_deref(), Some("2026-09-13 06:00"));
    }
}

#[cfg(test)]
mod release_clears_window_tests {
    use super::*;

    #[test]
    fn release_to_auto_membersihkan_penanda_window_supaya_bisa_dibuka_ulang() {
        // Reproduksi laporan bug: valve auto-open di window X (last_window=X), operator
        // mematikan manual (source=manual, is_open=false), lalu klik "Kembalikan ke Auto".
        // Sebelum fix: last_window masih X -> evaluate_valve menolak membuka lagi selama
        // masih di window X -> valve diam di OFF sampai window berikutnya.
        let mut twin = Twin::default();
        twin.set_schedule_window_ran("esp32-01", 1, "2026-09-13 06:00".to_string());
        twin.set_valve_state("esp32-01", 1, false, "manual", None);
        assert_eq!(twin.get("esp32-01").unwrap().valve_1_last_window.as_deref(), Some("2026-09-13 06:00"));

        twin.release_to_auto("esp32-01", 1);

        let state = twin.get("esp32-01").unwrap();
        assert_eq!(state.valve_1_source, "auto");
        assert_eq!(state.valve_1_last_window, None, "penanda window harus bersih supaya evaluate_valve boleh membuka lagi");
    }

    #[test]
    fn release_expired_manual_juga_membersihkan_penanda_window() {
        // Kedaluwarsa otomatis (expires_at lewat) harus punya efek yang sama dengan klik
        // release manual: mesin schedule dapat kendali penuh lagi.
        let mut twin = Twin::default();
        twin.set_schedule_window_ran("esp32-01", 2, "2026-09-13 17:00".to_string());
        twin.set_valve_state("esp32-01", 2, true, "manual", Some(1_000));

        twin.release_expired_manual("esp32-01", 1_001);

        let state = twin.get("esp32-01").unwrap();
        assert_eq!(state.valve_2_source, "auto");
        assert_eq!(state.valve_2_last_window, None);
    }

    #[test]
    fn release_valve_lain_tidak_ikut_membersihkan_penanda_valve_ini() {
        // Melepas valve 1 tidak boleh menyentuh penanda window valve 2.
        let mut twin = Twin::default();
        twin.set_schedule_window_ran("esp32-01", 1, "2026-09-13 06:00".to_string());
        twin.set_schedule_window_ran("esp32-01", 2, "2026-09-13 06:00".to_string());

        twin.release_to_auto("esp32-01", 1);

        let state = twin.get("esp32-01").unwrap();
        assert_eq!(state.valve_1_last_window, None);
        assert_eq!(state.valve_2_last_window.as_deref(), Some("2026-09-13 06:00"));
    }
}
