use serde::{Deserialize, Serialize};

/// Command Pi->ESP32 (topic `irigasi/ipesg/v1/<device_id>/command`), lihat plan.md §2.
#[derive(Debug, Clone, Serialize)]
pub struct ValveCommand {
    pub device_id: String,
    pub command_id: String,
    pub action: ValveAction,
    pub duration_s: u32,
    pub valve_id: u8,
    pub issued_at: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValveAction {
    Open,
    Close,
}
