use std::time::Duration;

use anyhow::Result;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};

use crate::config::MqttConfig;

pub fn connect(cfg: &MqttConfig) -> (AsyncClient, EventLoop) {
    let mut opts = MqttOptions::new(&cfg.client_id, &cfg.broker_host, cfg.broker_port);
    opts.set_keep_alive(Duration::from_secs(30));
    AsyncClient::new(opts, 10)
}

pub async fn subscribe_sensors(client: &AsyncClient) -> Result<()> {
    client
        .subscribe(super::topics::sensor_topic_filter(), QoS::AtLeastOnce)
        .await?;
    Ok(())
}
