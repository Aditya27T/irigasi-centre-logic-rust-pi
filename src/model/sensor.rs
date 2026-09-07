use serde::Deserialize;

/// Payload sensor dari ESP32 (topic `irigasi/ipesg/v1/<device_id>/sensor`).
/// Nama field wire (`soil3_pct`, `suhu3`, dst.) mengikuti firmware `iot.c` yang
/// sudah jalan di hardware -- lihat plan.md §2. valve 1 <-> pasangan sensor 3,
/// valve 2 <-> pasangan sensor 4.
#[derive(Debug, Clone, Deserialize)]
pub struct SensorReading {
    pub device_id: String,

    #[serde(rename = "soil3_pct")]
    pub soil_moisture_1_pct: f32,
    #[serde(rename = "soil4_pct")]
    pub soil_moisture_2_pct: f32,
    #[serde(rename = "suhu3")]
    pub temp_1_c: f32,
    #[serde(rename = "suhu4")]
    pub temp_2_c: f32,

    // Belum dikirim firmware saat ini -- opsional supaya tetap kompatibel.
    #[serde(default)]
    pub seq: Option<i64>,
    #[serde(default)]
    pub uptime_ms: Option<i64>,

    /// Status relay fisik aktual dari ESP32 ("ON"/"OFF"), sumber kebenaran untuk
    /// status valve di dashboard. Opsional supaya kompatibel dengan firmware lama.
    #[serde(default, rename = "relay3")]
    pub relay_1: Option<String>,
    #[serde(default, rename = "relay4")]
    pub relay_2: Option<String>,
}

impl SensorReading {
    pub fn relay_1_open(&self) -> Option<bool> {
        relay_status_to_bool(self.relay_1.as_deref())
    }

    pub fn relay_2_open(&self) -> Option<bool> {
        relay_status_to_bool(self.relay_2.as_deref())
    }
}

fn relay_status_to_bool(status: Option<&str>) -> Option<bool> {
    match status {
        Some("ON") => Some(true),
        Some("OFF") => Some(false),
        _ => None,
    }
}
