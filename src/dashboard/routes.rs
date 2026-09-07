use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::model::command::{ValveAction, ValveCommand};
use crate::mqtt::topics;
use crate::storage::repo;

use super::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_page))
        .route("/api/nodes", get(list_nodes))
        .route("/api/nodes/:device_id/config", get(node_config))
        .route("/api/nodes/:device_id/readings", get(node_readings))
        .route("/api/nodes/:device_id/decisions", get(node_decisions))
        .route("/api/nodes/:device_id/command", post(post_command))
        .route("/api/nodes/:device_id/valves/:valve_id/release", post(release_valve))
        .with_state(state)
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

enum ApiError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl ApiError {
    fn internal(err: impl std::fmt::Display) -> Self {
        ApiError::Internal(err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// Node dianggap online kalau reading terakhir masuk dalam rentang ini (detik).
/// ESP32 publish tiap ~2 detik, jadi 30 detik sudah cukup toleran untuk drop sesekali.
const ONLINE_THRESHOLD_S: i64 = 30;

#[derive(serde::Serialize)]
struct NodeSummary {
    device_id: String,
    zone: Option<String>,
    online: bool,
    last_soil_moisture_1_pct: Option<f32>,
    last_soil_moisture_2_pct: Option<f32>,
    last_temp_1_c: Option<f32>,
    last_temp_2_c: Option<f32>,
    valve_1_open: bool,
    valve_1_source: String,
    valve_1_expires_at: Option<i64>,
    valve_2_open: bool,
    valve_2_source: String,
    valve_2_expires_at: Option<i64>,
    water_balance_mm: f32,
    last_reading_at: Option<i64>,
}

async fn list_nodes(State(state): State<AppState>) -> Result<Json<Vec<NodeSummary>>, ApiError> {
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || repo::list_twin_states(&db.lock().unwrap()))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let mut by_device: std::collections::HashMap<String, repo::TwinStateRow> =
        rows.into_iter().map(|r| (r.device_id.clone(), r)).collect();
    let now = chrono::Utc::now().timestamp();

    let summaries = state
        .nodes
        .iter()
        .map(|node_cfg| match by_device.remove(&node_cfg.id) {
            Some(row) => NodeSummary {
                device_id: row.device_id,
                zone: node_cfg.zone.clone(),
                online: row
                    .last_reading_at
                    .is_some_and(|t| now - t <= ONLINE_THRESHOLD_S),
                last_soil_moisture_1_pct: row.last_soil_moisture_1_pct,
                last_soil_moisture_2_pct: row.last_soil_moisture_2_pct,
                last_temp_1_c: row.last_temp_1_c,
                last_temp_2_c: row.last_temp_2_c,
                valve_1_open: row.valve_1_open,
                valve_1_source: row.valve_1_source,
                valve_1_expires_at: row.valve_1_expires_at,
                valve_2_open: row.valve_2_open,
                valve_2_source: row.valve_2_source,
                valve_2_expires_at: row.valve_2_expires_at,
                water_balance_mm: row.water_balance_mm,
                last_reading_at: row.last_reading_at,
            },
            None => NodeSummary {
                device_id: node_cfg.id.clone(),
                zone: node_cfg.zone.clone(),
                online: false,
                last_soil_moisture_1_pct: None,
                last_soil_moisture_2_pct: None,
                last_temp_1_c: None,
                last_temp_2_c: None,
                valve_1_open: false,
                valve_1_source: "auto".to_string(),
                valve_1_expires_at: None,
                valve_2_open: false,
                valve_2_source: "auto".to_string(),
                valve_2_expires_at: None,
                water_balance_mm: 0.0,
                last_reading_at: None,
            },
        })
        .collect();

    Ok(Json(summaries))
}

async fn node_config(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<crate::config::NodeConfig>, ApiError> {
    state
        .nodes
        .iter()
        .find(|n| n.id == device_id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("node {device_id} tidak ditemukan")))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn node_readings(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<repo::ReadingRow>>, ApiError> {
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || repo::list_readings(&db.lock().unwrap(), &device_id, query.limit))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

async fn node_decisions(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<repo::DecisionRow>>, ApiError> {
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || repo::list_decisions(&db.lock().unwrap(), &device_id, query.limit))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct ManualCommandRequest {
    valve_id: u8,
    action: ValveAction,
    #[serde(default)]
    duration_s: u32,
}

#[derive(serde::Serialize)]
struct ManualCommandResponse {
    command_id: String,
    published: bool,
}

async fn post_command(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(req): Json<ManualCommandRequest>,
) -> Result<Json<ManualCommandResponse>, ApiError> {
    if !state.nodes.iter().any(|n| n.id == device_id) {
        return Err(ApiError::NotFound(format!("node {device_id} tidak ditemukan")));
    }
    if req.valve_id != 1 && req.valve_id != 2 {
        return Err(ApiError::BadRequest("valve_id harus 1 atau 2".to_string()));
    }
    if req.action == ValveAction::Open && req.duration_s == 0 {
        return Err(ApiError::BadRequest(
            "duration_s wajib > 0 untuk action open".to_string(),
        ));
    }

    let command_id = uuid::Uuid::new_v4().to_string();
    let command = ValveCommand {
        device_id: device_id.clone(),
        command_id: command_id.clone(),
        action: req.action,
        duration_s: req.duration_s,
        valve_id: req.valve_id,
        issued_at: chrono::Utc::now().timestamp(),
        reason: "manual_override".to_string(),
    };
    let topic = topics::command_topic(&device_id);
    let payload = serde_json::to_vec(&command).map_err(ApiError::internal)?;

    let published = state
        .mqtt
        .publish(&topic, rumqttc::QoS::AtLeastOnce, false, payload)
        .await
        .is_ok();

    let db = state.db.clone();
    let state_twin = state.twin.clone();
    let now = chrono::Utc::now().timestamp();
    let device_id_for_blocking = device_id.clone();
    let cmd_id_for_blocking = command_id.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().unwrap();
        if let Err(err) = repo::insert_manual_decision(
            &conn,
            &device_id_for_blocking,
            req.valve_id,
            req.action,
            req.duration_s,
            &cmd_id_for_blocking,
            published,
            now,
        ) {
            tracing::error!("Gagal simpan manual decision: {err}");
        }
        if published {
            let is_open = req.action == ValveAction::Open;
            let expires_at = if is_open { Some(now + req.duration_s as i64) } else { None };
            if let Err(err) = repo::update_twin_valve_state(
                &conn,
                &device_id_for_blocking,
                req.valve_id,
                is_open,
                "manual",
                expires_at,
                now,
            ) {
                tracing::error!("Gagal update twin_state setelah manual override: {err}");
            }
            // Update juga twin in-memory yang dipakai decision engine di loop MQTT,
            // supaya engine langsung berhenti mengevaluasi valve ini sampai dilepas
            // lagi ke auto -- kalau tidak, upsert_twin_state() di reading berikutnya
            // akan menimpa balik status manual ini.
            state_twin.lock().unwrap().set_valve_state(
                &device_id_for_blocking,
                req.valve_id,
                is_open,
                "manual",
                expires_at,
            );
        }
    })
    .await
    .map_err(ApiError::internal)?;

    Ok(Json(ManualCommandResponse { command_id, published }))
}

#[derive(serde::Serialize)]
struct ReleaseResponse {
    ok: bool,
}

async fn release_valve(
    State(state): State<AppState>,
    Path((device_id, valve_id)): Path<(String, u8)>,
) -> Result<Json<ReleaseResponse>, ApiError> {
    if !state.nodes.iter().any(|n| n.id == device_id) {
        return Err(ApiError::NotFound(format!("node {device_id} tidak ditemukan")));
    }
    if valve_id != 1 && valve_id != 2 {
        return Err(ApiError::BadRequest("valve_id harus 1 atau 2".to_string()));
    }

    let db = state.db.clone();
    let now = chrono::Utc::now().timestamp();
    let device_id_for_blocking = device_id.clone();
    tokio::task::spawn_blocking(move || {
        repo::release_valve_to_auto(&db.lock().unwrap(), &device_id_for_blocking, valve_id, now)
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)?;

    state.twin.lock().unwrap().release_to_auto(&device_id, valve_id);

    Ok(Json(ReleaseResponse { ok: true }))
}
