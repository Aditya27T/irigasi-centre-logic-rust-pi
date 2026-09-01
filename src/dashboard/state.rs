use std::sync::{Arc, Mutex};

use rumqttc::AsyncClient;
use rusqlite::Connection;

use crate::config::NodeConfig;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub mqtt: AsyncClient,
    pub nodes: Arc<Vec<NodeConfig>>,
}
