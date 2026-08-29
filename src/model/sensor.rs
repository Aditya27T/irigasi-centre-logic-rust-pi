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
}
