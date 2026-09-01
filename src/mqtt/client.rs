use std::time::Duration;

use anyhow::Result;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, NetworkOptions, QoS, TlsConfiguration, Transport};

use crate::config::MqttConfig;

pub fn connect(cfg: &MqttConfig) -> (AsyncClient, EventLoop) {
    let mut opts = MqttOptions::new(&cfg.client_id, &cfg.broker_host, cfg.broker_port);
    opts.set_keep_alive(Duration::from_secs(30));

    if let (Some(username), Some(password)) = (&cfg.username, &cfg.password) {
        opts.set_credentials(username, password);
    }

    if cfg.use_tls {
        // TlsConfiguration::default() memakai native cert store (rustls-native-certs)
        // -- cukup untuk broker publik dengan sertifikat CA umum seperti HiveMQ Cloud.
        opts.set_transport(Transport::Tls(TlsConfiguration::default()));
    }

    let (client, mut eventloop) = AsyncClient::new(opts, 10);

    if cfg.use_tls {
        // Default 5s terlalu ketat untuk TLS handshake + auth roundtrip ke broker cloud
        // (native cert store loading di macOS juga menambah latensi) -- longgarkan ke 15s.
        let mut net_opts = NetworkOptions::new();
        net_opts.set_connection_timeout(15);
        eventloop.set_network_options(net_opts);
    }

    (client, eventloop)
}

pub async fn subscribe_sensors(client: &AsyncClient) -> Result<()> {
    client
        .subscribe(super::topics::sensor_topic_filter(), QoS::AtLeastOnce)
        .await?;
    Ok(())
}
