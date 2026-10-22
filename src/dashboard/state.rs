use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use rumqttc::AsyncClient;

use crate::config::SharedNodes;
use crate::twin::Twin;

/// Token sesi login yang sedang aktif. Sengaja hanya di memori (bukan DB): ini autentikasi
/// dummy satu-akun sesuai permintaan, bukan sistem produksi -- semua sesi otomatis batal
/// kalau service di-restart, operator tinggal login ulang. Lihat `dashboard::auth`.
pub type SessionStore = Arc<Mutex<HashSet<String>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub mqtt: AsyncClient,
    pub nodes: SharedNodes,
    pub twin: Arc<Mutex<Twin>>,
    pub sessions: SessionStore,
}
