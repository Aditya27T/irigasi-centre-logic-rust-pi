//! Konvensi topic sesuai plan.md §2: irigasi/ipesg/v1/<device_id>/...
//! ("v1" = versi skema, bukan ID node).

pub fn sensor_topic_filter() -> &'static str {
    "irigasi/ipesg/v1/+/sensor"
}

pub fn command_topic(device_id: &str) -> String {
    format!("irigasi/ipesg/v1/{device_id}/command")
}

/// Ekstrak device_id dari topic sensor masuk, mis.
/// "irigasi/ipesg/v1/esp32-02/sensor" -> Some("esp32-02").
pub fn device_id_from_sensor_topic(topic: &str) -> Option<&str> {
    let parts: Vec<&str> = topic.split('/').collect();
    match parts.as_slice() {
        ["irigasi", "ipesg", _version, device_id, "sensor"] => Some(device_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_id_from_sensor_topic() {
        assert_eq!(
            device_id_from_sensor_topic("irigasi/ipesg/v1/esp32-02/sensor"),
            Some("esp32-02")
        );
        assert_eq!(device_id_from_sensor_topic("irigasi/ipesg/v1/esp32-02/command"), None);
        assert_eq!(device_id_from_sensor_topic("something/else"), None);
    }

    #[test]
    fn builds_command_topic() {
        assert_eq!(command_topic("esp32-02"), "irigasi/ipesg/v1/esp32-02/command");
    }
}
