//! Autentikasi dummy: satu akun hardcoded (admin/admin123), sesi token acak disimpan di
//! memori. Ini SENGAJA sederhana -- tidak ada hashing password, tidak ada multi-user, tidak
//! ada rate limiting, dan sesi hilang total kalau service restart. Cukup untuk menutup akses
//! anonim sekarang dashboard sudah publik lewat Cloudflare Tunnel; bukan pengganti Cloudflare
//! Access (Zero Trust) yang lebih layak untuk jangka panjang.

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use super::state::{AppState, SessionStore};

const SESSION_COOKIE: &str = "irigasi_session";
const USERNAME: &str = "admin";
const PASSWORD: &str = "admin123";

pub async fn login_page() -> Html<&'static str> {
    Html(include_str!("../../static/login.html"))
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

pub async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> Response {
    if req.username != USERNAME || req.password != PASSWORD {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "username atau password salah"})),
        )
            .into_response();
    }
    let token = uuid::Uuid::new_v4().to_string();
    state.sessions.lock().unwrap().insert(token.clone());
    (
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie(&token, 86_400))],
        Json(json!({"ok": true})),
    )
        .into_response()
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = extract_session_token(&headers) {
        state.sessions.lock().unwrap().remove(&token);
    }
    (
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie("", 0))],
        Json(json!({"ok": true})),
    )
        .into_response()
}

/// `Max-Age=0` menghapus cookie di browser -- dipakai baik untuk set token baru (dengan
/// `max_age_s` panjang) maupun untuk logout (`max_age_s = 0`, token kosong).
/// Tanpa flag `Secure` dengan sengaja: dashboard diakses baik lewat HTTPS (Cloudflare Tunnel)
/// maupun HTTP langsung di LAN (`http://<ip-pi>:8080`) -- `Secure` akan membuat cookie tidak
/// terkirim sama sekali di jalur LAN yang plain HTTP.
fn session_cookie(token: &str, max_age_s: i64) -> String {
    format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_s}")
}

fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == SESSION_COOKIE).then(|| value.to_string())
    })
}

fn is_valid_session(headers: &HeaderMap, sessions: &SessionStore) -> bool {
    match extract_session_token(headers) {
        Some(token) => sessions.lock().unwrap().contains(&token),
        None => false,
    }
}

/// Middleware yang dipasang lewat `route_layer` HANYA pada router terproteksi (lihat
/// `build_router`) -- `/login`, `/api/login`, `/api/logout` sengaja tidak melewati fungsi ini
/// sama sekali karena harus tetap bisa diakses tanpa sesi.
pub async fn require_session(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if is_valid_session(request.headers(), &state.sessions) {
        return next.run(request).await;
    }
    // Halaman HTML (GET /) diarahkan ke form login; endpoint /api/* (dipanggil lewat fetch,
    // termasuk saat sesi kedaluwarsa di tengah dashboard terbuka) mendapat 401 JSON supaya
    // JavaScript bisa menampilkan pesan, bukan mem-parse HTML sebagai JSON.
    if request.uri().path() == "/" {
        Redirect::to("/login").into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "sesi login tidak valid, silakan login ulang"})),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    fn headers_with_cookie(cookie: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, cookie.parse().unwrap());
        h
    }

    #[test]
    fn extract_session_token_mengambil_dari_beberapa_cookie() {
        let h = headers_with_cookie("foo=bar; irigasi_session=abc123; baz=qux");
        assert_eq!(extract_session_token(&h).as_deref(), Some("abc123"));
    }

    #[test]
    fn extract_session_token_none_kalau_tidak_ada_cookie_header() {
        assert_eq!(extract_session_token(&HeaderMap::new()), None);
    }

    #[test]
    fn extract_session_token_none_kalau_nama_cookie_tidak_cocok() {
        let h = headers_with_cookie("lain=abc123");
        assert_eq!(extract_session_token(&h), None);
    }

    #[test]
    fn is_valid_session_true_hanya_untuk_token_yang_terdaftar() {
        let sessions: SessionStore = Arc::new(Mutex::new(HashSet::from(["tokenA".to_string()])));
        assert!(is_valid_session(&headers_with_cookie("irigasi_session=tokenA"), &sessions));
        assert!(!is_valid_session(&headers_with_cookie("irigasi_session=tokenB"), &sessions));
        assert!(!is_valid_session(&HeaderMap::new(), &sessions));
    }

    #[test]
    fn session_cookie_max_age_nol_untuk_logout() {
        let c = session_cookie("", 0);
        assert!(c.contains("Max-Age=0"));
        assert!(c.starts_with("irigasi_session=;"));
    }
}
