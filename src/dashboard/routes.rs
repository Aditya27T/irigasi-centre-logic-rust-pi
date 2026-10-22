use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::config::{NodeConfig, ValveConfig};
use crate::model::command::{ValveAction, ValveCommand};
use crate::mqtt::topics;
use crate::storage::repo;

use super::auth;
use super::AppState;

pub fn build_router(state: AppState) -> Router {
    // Rute publik: HARUS bisa diakses tanpa sesi supaya login itu sendiri mungkin dilakukan.
    let public = Router::new()
        .route("/login", get(auth::login_page))
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout));

    // Semua rute lain wajib sesi valid. `route_layer` hanya membungkus rute yang SUDAH
    // didaftarkan di router ini -- digabung ke `public` sesudahnya supaya /login dkk. di atas
    // tidak ikut tersentuh middleware ini.
    let protected = Router::new()
        .route("/", get(index_page))
        .route("/api/nodes", get(list_nodes).post(create_node))
        .route("/api/settings", get(get_settings))
        .route("/api/schedule/active", get(active_schedule))
        .route("/api/nodes/:device_id/config", get(node_config).put(put_node_config))
        .route("/api/nodes/:device_id/deactivate", post(deactivate_node))
        .route("/api/nodes/:device_id/activate", post(activate_node))
        .route("/api/nodes/:device_id/readings", get(node_readings))
        .route("/api/nodes/:device_id/decisions", get(node_decisions))
        .route("/api/nodes/:device_id/command", post(post_command))
        .route("/api/nodes/:device_id/valves/:valve_id/release", post(release_valve))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_session));

    public.merge(protected).with_state(state)
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

enum ApiError {
    NotFound(String),
    BadRequest(String),
    Conflict(String),
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
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

const ONLINE_THRESHOLD_S: i64 = 30;

#[derive(serde::Serialize)]
struct NodeSummary {
    device_id: String,
    zone: Option<String>,
    enabled: bool,
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
    last_reading_at: Option<i64>,
}

#[derive(Deserialize)]
struct ListNodesQuery {
    #[serde(default)]
    include_disabled: Option<bool>,
}

async fn list_nodes(State(state): State<AppState>, Query(q): Query<ListNodesQuery>) -> Result<Json<Vec<NodeSummary>>, ApiError> {
    let include_disabled = q.include_disabled.unwrap_or(false);
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || repo::list_twin_states(&db.lock().unwrap()))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let mut by_device: std::collections::HashMap<String, repo::TwinStateRow> =
        rows.into_iter().map(|r| (r.device_id.clone(), r)).collect();
    let now = chrono::Utc::now().timestamp();

    let guard = state.nodes.read().unwrap();
    let summaries: Vec<NodeSummary> = guard
        .iter()
        .filter(|n| include_disabled || n.enabled)
        .map(|node_cfg| match by_device.remove(&node_cfg.id) {
            Some(row) => NodeSummary {
                device_id: row.device_id,
                zone: node_cfg.zone.clone(),
                enabled: node_cfg.enabled,
                online: row.last_reading_at.is_some_and(|t| now - t <= ONLINE_THRESHOLD_S),
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
                last_reading_at: row.last_reading_at,
            },
            None => NodeSummary {
                device_id: node_cfg.id.clone(),
                zone: node_cfg.zone.clone(),
                enabled: node_cfg.enabled,
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
                last_reading_at: None,
            },
        })
        .collect();

    Ok(Json(summaries))
}

async fn node_config(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<NodeConfig>, ApiError> {
    let guard = state.nodes.read().unwrap();
    guard
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
    if !state.nodes.read().unwrap().iter().any(|n| n.id == device_id) {
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
    if !state.nodes.read().unwrap().iter().any(|n| n.id == device_id) {
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

#[derive(Deserialize)]
struct NodeConfigRequest {
    #[serde(default)]
    zone: Option<String>,
    valve_1: serde_json::Value,
    valve_2: serde_json::Value,
}

async fn put_node_config(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(req): Json<NodeConfigRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let valve_1: ValveConfig = serde_json::from_value(req.valve_1).map_err(|e| ApiError::BadRequest(format!("valve_1 tidak valid: {e}")))?;
    let valve_2: ValveConfig = serde_json::from_value(req.valve_2).map_err(|e| ApiError::BadRequest(format!("valve_2 tidak valid: {e}")))?;
    let zone = req.zone.map(|z| z.trim().to_string()).filter(|z| !z.is_empty());
    if let Some(ref z) = zone { if z.len() > 64 { return Err(ApiError::BadRequest("zone maksimal 64 karakter".to_string())); } }

    let existing_enabled = {
        let guard = state.nodes.read().unwrap();
        guard.iter().find(|n| n.id == device_id).map(|n| n.enabled).ok_or_else(|| ApiError::NotFound(format!("node {device_id} tidak ditemukan")))?
    };

    let updated = NodeConfig { id: device_id.clone(), zone, valve_1, valve_2, enabled: existing_enabled };
    updated.validate().map_err(ApiError::BadRequest)?;

    let db = state.db.clone();
    let nodes = state.nodes.clone();
    let updated_clone = updated.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut conn = db.lock().unwrap();
        crate::storage::settings::upsert_node(&mut conn, &updated_clone)?;
        let mut guard = nodes.write().unwrap();
        match guard.iter_mut().find(|n| n.id == updated_clone.id) {
            Some(slot) => *slot = updated_clone,
            None => { guard.push(updated_clone); guard.sort_by(|a, b| a.id.cmp(&b.id)); }
        }
        Ok(())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)?;

    Ok(Json(serde_json::json!({ "ok": true, "node": updated })))
}

#[derive(Deserialize)]
struct CreateNodeRequest {
    id: String,
    #[serde(default)]
    zone: Option<String>,
    valve_1: Option<serde_json::Value>,
    valve_2: Option<serde_json::Value>,
}

async fn create_node(
    State(state): State<AppState>,
    Json(req): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let id = req.id.trim().to_string();
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(ApiError::BadRequest("id tidak valid (A-Z a-z 0-9 _ - , max 64)".to_string()));
    }
    {
        let guard = state.nodes.read().unwrap();
        if guard.iter().any(|n| n.id == id) {
            return Err(ApiError::Conflict(format!("node {id} sudah ada")));
        }
    }
    let zone = req.zone.map(|z| z.trim().to_string()).filter(|z| !z.is_empty());
    if let Some(ref z) = zone { if z.len() > 64 { return Err(ApiError::BadRequest("zone maksimal 64 karakter".to_string())); } }
    let default_valve = ValveConfig::Schedule { close_temp_threshold_c: 25.0, close_soil_min_pct: 45.0, max_open_duration_s: 120, schedule: vec![] };
    let valve_1 = if let Some(v) = req.valve_1 { serde_json::from_value(v).map_err(|e| ApiError::BadRequest(format!("valve_1 tidak valid: {e}")))? } else { default_valve.clone() };
    let valve_2 = if let Some(v) = req.valve_2 { serde_json::from_value(v).map_err(|e| ApiError::BadRequest(format!("valve_2 tidak valid: {e}")))? } else { default_valve };
    let node = NodeConfig { id: id.clone(), zone, valve_1, valve_2, enabled: true };
    node.validate().map_err(ApiError::BadRequest)?;

    let db = state.db.clone();
    let nodes = state.nodes.clone();
    let node_clone = node.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut conn = db.lock().unwrap();
        crate::storage::settings::upsert_node(&mut conn, &node_clone)?;
        let mut guard = nodes.write().unwrap();
        guard.push(node_clone);
        guard.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(())
    })
    .await.map_err(ApiError::internal)?.map_err(ApiError::internal)?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "ok": true, "node": node }))))
}

async fn deactivate_node(State(state): State<AppState>, Path(device_id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    {
        let twin = state.twin.lock().unwrap();
        if let Some(s) = twin.get(&device_id) {
            if s.valve_1_open || s.valve_2_open {
                let which = if s.valve_1_open && s.valve_2_open { "valve 1 dan 2" } else if s.valve_1_open { "valve 1" } else { "valve 2" };
                return Err(ApiError::BadRequest(format!("{which} masih terbuka, tutup dulu sebelum menonaktifkan node")));
            }
        }
    }
    {
        let guard = state.nodes.read().unwrap();
        if guard.iter().find(|n| n.id == device_id).is_none() {
            return Err(ApiError::NotFound(format!("node {device_id} tidak ditemukan")));
        }
    }
    let db = state.db.clone();
    let nodes = state.nodes.clone();
    let did = device_id.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = db.lock().unwrap();
        crate::storage::settings::set_node_enabled(&conn, &did, false)?;
        let mut guard = nodes.write().unwrap();
        if let Some(n) = guard.iter_mut().find(|n| n.id == did) { n.enabled = false; }
        Ok(())
    }).await.map_err(ApiError::internal)?.map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn activate_node(State(state): State<AppState>, Path(device_id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    {
        let guard = state.nodes.read().unwrap();
        if guard.iter().find(|n| n.id == device_id).is_none() {
            return Err(ApiError::NotFound(format!("node {device_id} tidak ditemukan")));
        }
    }
    let db = state.db.clone();
    let nodes = state.nodes.clone();
    let did = device_id.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = db.lock().unwrap();
        crate::storage::settings::set_node_enabled(&conn, &did, true)?;
        let mut guard = nodes.write().unwrap();
        if let Some(n) = guard.iter_mut().find(|n| n.id == did) { n.enabled = true; }
        Ok(())
    }).await.map_err(ApiError::internal)?.map_err(ApiError::internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_settings(State(state): State<AppState>) -> Json<Vec<NodeConfig>> {
    let guard = state.nodes.read().unwrap();
    Json(guard.clone())
}

#[derive(serde::Serialize)]
struct ActiveScheduleResponse {
    now: String,
    now_epoch: i64,
    items: Vec<ActiveScheduleItem>,
}

#[derive(serde::Serialize)]
struct ActiveScheduleItem {
    device_id: String,
    zone: Option<String>,
    valve_id: u8,
    mode: String,
    state: String,
    window: Option<ScheduleWindowDto>,
    seconds_remaining: Option<i64>,
    seconds_until_start: Option<i64>,
    tomorrow: bool,
}

#[derive(serde::Serialize)]
struct ScheduleWindowDto {
    start: String,
    end: String,
}

async fn active_schedule(State(state): State<AppState>) -> Json<ActiveScheduleResponse> {
    let now_time = chrono::Local::now().time();
    let now_epoch = chrono::Utc::now().timestamp();
    let now_str = now_time.format("%H:%M").to_string();
    let guard = state.nodes.read().unwrap();
    let mut items = Vec::new();
    for node in guard.iter().filter(|n| n.enabled) {
        for (valve_id, cfg) in [(1, &node.valve_1), (2, &node.valve_2)] {
            if let ValveConfig::Schedule { schedule, .. } = cfg {
                if schedule.is_empty() { continue; }
                if let Some((win, running)) = crate::decision::rules::active_or_next_window(now_time, schedule) {
                    let start = crate::config::parse_hhmm(&win.start).unwrap();
                    let end = crate::config::parse_hhmm(&win.end).unwrap();
                    let (seconds_remaining, seconds_until_start, tomorrow) = if running {
                        let rem = (end - now_time).num_seconds();
                        (Some(rem), None, false)
                    } else {
                        let until = if start > now_time { (start - now_time).num_seconds() } else { (chrono::NaiveTime::from_hms_opt(24,0,0).unwrap() - now_time).num_seconds() + (start - chrono::NaiveTime::from_hms_opt(0,0,0).unwrap()).num_seconds() };
                        let is_tomorrow = start <= now_time;
                        (None, Some(until), is_tomorrow)
                    };
                    let state_str = if running { "running" } else if seconds_until_start.is_some() { "upcoming" } else { "none" };
                    items.push(ActiveScheduleItem {
                        device_id: node.id.clone(),
                        zone: node.zone.clone(),
                        valve_id,
                        mode: "schedule".to_string(),
                        state: state_str.to_string(),
                        window: Some(ScheduleWindowDto { start: win.start.clone(), end: win.end.clone() }),
                        seconds_remaining,
                        seconds_until_start,
                        tomorrow,
                    });
                }
            }
        }
    }
    Json(ActiveScheduleResponse { now: now_str, now_epoch, items })
}
