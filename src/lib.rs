pub mod config;
#[cfg(feature = "dashboard")]
pub mod dashboard;
pub mod decision;
pub mod model;
pub mod mqtt;
pub mod storage;
pub mod twin;

use anyhow::{Context, Result};
use chrono::Local;
use rumqttc::{Event, Packet, QoS};
use tracing::{error, info, warn};

use config::Config;
use decision::rules::Action;
use model::command::{ValveAction, ValveCommand};
use model::sensor::SensorReading;
use twin::Twin;

pub async fn run(config: Config) -> Result<()> {
    let db = storage::db::open(&config.storage.sqlite_path).context("membuka database SQLite")?;

    let (client, mut eventloop) = mqtt::client::connect(&config.mqtt);
    mqtt::client::subscribe_sensors(&client).await?;
    info!(
        "Terhubung ke MQTT broker {}:{}, subscribe {}",
        config.mqtt.broker_host,
        config.mqtt.broker_port,
        mqtt::topics::sensor_topic_filter()
    );

    let mut twin = Twin::default();

    #[cfg(feature = "dashboard")]
    if config.dashboard.enabled {
        let dashboard_db = storage::db::open(&config.storage.sqlite_path)
            .context("membuka koneksi SQLite kedua untuk dashboard")?;
        let state = dashboard::AppState {
            db: std::sync::Arc::new(std::sync::Mutex::new(dashboard_db)),
            mqtt: client.clone(),
            nodes: std::sync::Arc::new(config.nodes.clone()),
        };
        let bind_addr = config.dashboard.bind_addr.clone();
        tokio::spawn(async move {
            if let Err(err) = dashboard::serve(state, &bind_addr).await {
                error!("Dashboard HTTP server berhenti dengan error: {err}");
            }
        });
    }

    loop {
        let event = match eventloop.poll().await {
            Ok(event) => event,
            Err(err) => {
                warn!("MQTT eventloop error: {err} (rumqttc akan reconnect otomatis)");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let Event::Incoming(Packet::Publish(publish)) = event else {
            continue;
        };

        let Some(device_id) = mqtt::topics::device_id_from_sensor_topic(&publish.topic) else {
            continue;
        };
        let device_id = device_id.to_string();

        let reading: SensorReading = match serde_json::from_slice(&publish.payload) {
            Ok(r) => r,
            Err(err) => {
                warn!("Gagal parse payload sensor dari topic {}: {err}", publish.topic);
                continue;
            }
        };

        let received_at = chrono::Utc::now().timestamp();
        if let Err(err) = storage::repo::insert_reading(&db, &reading, received_at) {
            error!("Gagal simpan reading: {err}");
        }

        twin.update_from_reading(&reading, received_at);

        let Some(node_cfg) = config.nodes.iter().find(|n| n.id == device_id) else {
            warn!("Tidak ada konfigurasi node untuk device_id={device_id}, reading dicatat tapi tidak dievaluasi");
            continue;
        };

        let zone_state = twin.get(&device_id).cloned().unwrap_or_default();
        let now = Local::now().time();
        let outcomes = decision::rules::evaluate(&reading, node_cfg, &zone_state, now);

        for outcome in outcomes {
            let command_id = if outcome.action != Action::None {
                Some(uuid::Uuid::new_v4().to_string())
            } else {
                None
            };

            let mut published = false;
            if outcome.action != Action::None && !config.decision.dry_run {
                let command = ValveCommand {
                    device_id: device_id.clone(),
                    command_id: command_id.clone().expect("command_id ada untuk action != None"),
                    action: if outcome.action == Action::Open {
                        ValveAction::Open
                    } else {
                        ValveAction::Close
                    },
                    duration_s: outcome.duration_s.unwrap_or(0),
                    valve_id: outcome.valve_id,
                    issued_at: chrono::Utc::now().timestamp(),
                    reason: outcome.reason.to_string(),
                };
                let topic = mqtt::topics::command_topic(&device_id);

                match serde_json::to_vec(&command) {
                    Ok(payload) => match client.publish(&topic, QoS::AtLeastOnce, false, payload).await {
                        Ok(_) => {
                            published = true;
                            let is_open = outcome.action == Action::Open;
                            let expires_at = if is_open {
                                Some(chrono::Utc::now().timestamp() + outcome.duration_s.unwrap_or(0) as i64)
                            } else {
                                None
                            };
                            twin.set_valve_state(&device_id, outcome.valve_id, is_open, "auto", expires_at);
                            info!(
                                "Command dikirim: {topic} action={:?} valve_id={} reason={}",
                                outcome.action, outcome.valve_id, outcome.reason
                            );
                        }
                        Err(err) => error!("Gagal publish command ke {topic}: {err}"),
                    },
                    Err(err) => error!("Gagal serialize command: {err}"),
                }
            } else if outcome.action != Action::None && config.decision.dry_run {
                info!(
                    "[dry_run] Command TIDAK dikirim: device_id={device_id} action={:?} valve_id={} reason={}",
                    outcome.action, outcome.valve_id, outcome.reason
                );
            }

            if let Err(err) = storage::repo::insert_decision(
                &db,
                &device_id,
                command_id.as_deref(),
                &outcome,
                published,
                chrono::Utc::now().timestamp(),
            ) {
                error!("Gagal simpan decision: {err}");
            }
        }

        if let Some(state) = twin.get(&device_id) {
            if let Err(err) = storage::repo::upsert_twin_state(&db, &device_id, state, chrono::Utc::now().timestamp()) {
                error!("Gagal update twin_state: {err}");
            }
        }
    }
}
