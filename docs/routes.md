# Rute Aplikasi — Irigasi Digital Twin (`irigasi-core`)

> **Status dokumen: DIHASILKAN DARI KODE, bukan spesifikasi awal.**
> Sumber: `src/dashboard/routes.rs` (`build_router`), `src/dashboard/auth.rs`, `src/dashboard/mod.rs`.
> Dibuat otomatis untuk keperluan pengujian fungsional + screenshot anotasi.

## Ringkasan

Server dashboard memakai **axum 0.7** (`src/dashboard/mod.rs:7`). Router dibangun di
`build_router` (`src/dashboard/routes.rs:19`) dan hanya aktif bila:

1. binary dibangun dengan feature Cargo `dashboard`, **dan**
2. `[dashboard] enabled = true` di file config.

Default `DashboardConfig` adalah `enabled: false`, `bind_addr: "0.0.0.0:8080"`
(`src/config.rs:250-268`). Contoh aktif: `config/default.toml` →
`bind_addr = "127.0.0.1:8080"`.

## Peta Rute

| # | Method | Path | Handler | Auth | Keterangan |
|---|--------|------|---------|------|------------|
| 1 | GET | `/login` | `auth::login_page` | Publik | Halaman login (`static/login.html`) |
| 2 | POST | `/api/login` | `auth::login` | Publik | Set cookie sesi |
| 3 | POST | `/api/logout` | `auth::logout` | Publik | Hapus sesi + cookie |
| 4 | GET | `/` | `index_page` | **Sesi** | Dashboard SPA (`static/index.html`) |
| 5 | GET | `/api/nodes` | `list_nodes` | **Sesi** | Ringkasan node + state valve |
| 6 | POST | `/api/nodes` | `create_node` | **Sesi** | Daftarkan node baru → `201` |
| 7 | GET | `/api/settings` | `get_settings` | **Sesi** | Semua `NodeConfig` |
| 8 | GET | `/api/schedule/active` | `active_schedule` | **Sesi** | Jadwal aktif/berikutnya |
| 9 | GET | `/api/nodes/:device_id/config` | `node_config` | **Sesi** | Config satu node |
| 10 | PUT | `/api/nodes/:device_id/config` | `put_node_config` | **Sesi** | Simpan config (zone + valve_1 + valve_2) |
| 11 | POST | `/api/nodes/:device_id/deactivate` | `deactivate_node` | **Sesi** | Nonaktifkan node |
| 12 | POST | `/api/nodes/:device_id/activate` | `activate_node` | **Sesi** | Aktifkan node |
| 13 | GET | `/api/nodes/:device_id/readings` | `node_readings` | **Sesi** | Histori sensor, `?limit=` (default 50) |
| 14 | GET | `/api/nodes/:device_id/decisions` | `node_decisions` | **Sesi** | Histori keputusan, `?limit=` (default 50) |
| 15 | POST | `/api/nodes/:device_id/command` | `post_command` | **Sesi** | Override manual valve |
| 16 | POST | `/api/nodes/:device_id/valves/:valve_id/release` | `release_valve` | **Sesi** | Kembalikan valve ke mode auto |

### Halaman HTML yang di-render

| Path | File statis | Di-embed via |
|------|-------------|---------------|
| `/login` | `static/login.html` | `include_str!` di `auth.rs:22` |
| `/` | `static/index.html` | `include_str!` di `routes.rs:47` |

Keduanya **di-embed saat compile**, bukan dibaca dari disk saat runtime.

## Model Autentikasi

Satu akun hardcoded (`src/dashboard/auth.rs:18-19`):

| Field | Nilai |
|-------|-------|
| Username | `admin` |
| Password | `admin123` |
| Nama cookie | `irigasi_session` |
| Atribut cookie | `Path=/; HttpOnly; SameSite=Lax; Max-Age=86400` |

Token sesi = UUID v4, disimpan **hanya di memori** (`SessionStore = Arc<Mutex<HashSet<String>>>`,
`src/dashboard/state.rs:11`). Konsekuensi:

- Semua sesi batal saat service restart.
- Multi-user tidak didukung; login berulang menghasilkan token baru tidak menginvalidasi token lama.
- Password **tidak** di-hash. Komentar di `auth.rs:1-5` menyatakan ini disengaja — cukup sebagai
  lapisan di depan Cloudflare Tunnel, bukan pengganti Cloudflare Access.

### Perilaku middleware `require_session`

`require_session` dipasang via `route_layer` **hanya** pada router terproteksi
(`routes.rs:41`), lalu digabung dengan router publik *setelah* itu. Karena itu `/login`,
`/api/login`, `/api/logout` tidak pernah menyentuh middleware.

Saat token tidak valid (`auth.rs:88-108`):

| Request | Respons |
|---------|---------|
| `GET /` (path persis `/`) | `302` → `/login` |
| Semua `/api/*` | `401` + `{"error":"sesi login tidak valid, silakan login ulang"}` |

Kasus `401` sengaja dibedakan agar JavaScript tidak mencoba parse HTML sebagai JSON.
Fungsi `loadNodes()` yang menangkap `401` lalu `location.href = '/login'`
(`static/index.html:2056-2058`).

## Detail Per Endpoint

### 5. `GET /api/nodes`

Query: `include_disabled` (bool, default `false`).

Mengembalikan array `NodeSummary` (`routes.rs:77-94`). Sumber data adalah
`state.nodes` (in-memory, dari SQLite `node_settings`) di-join dengan `twin_state`.

Field penting:

| Field | Tipe | Sumber / Aturan |
|-------|------|-----------------|
| `device_id` | string | id node |
| `zone` | string? | opsional, `null` bila kosong |
| `enabled` | bool | kolom `node_settings.enabled` |
| `online` | bool | `last_reading_at` ada **dan** `now - last_reading_at <= 30` |
| `last_soil_moisture_1_pct` / `_2_pct` | f32? | reading terakhir |
| `last_temp_1_c` / `_2_c` | f32? | reading terakhir |
| `valve_1_open` / `valve_2_open` | bool | `twin_state` |
| `valve_1_source` / `valve_2_source` | string | `"auto"` \| `"manual"` |
| `valve_1_expires_at` / `_2_` | i64? | epoch detik, hanya bila open |
| `last_reading_at` | i64? | epoch detik |

`ONLINE_THRESHOLD_S = 30` (`routes.rs:75`). Node yang belum pernah mengirim reading
tetap dikembalikan dengan `online: false` dan seluruh nilai sensor `null`
(`routes.rs:136-152`).

### 6. `POST /api/nodes`

Body: `{ id: string, zone?: string, valve_1?: object, valve_2?: object }`

Validasi `id` (`routes.rs:397`): tidak kosong, ≤ 64 karakter, hanya `A-Z a-z 0-9 _ -`.

Bila `valve_1`/`valve_2` tidak dikirim, dipakai default
(`routes.rs:408`): `mode = schedule`, `close_temp_threshold_c = 25.0`,
`close_soil_min_pct = 45.0`, `max_open_duration_s = 120`, `schedule = []`.

Respons sukses: `201` + `{"ok": true, "node": {...}}`.

| Kondisi | Status |
|---------|--------|
| Sukses | `201` |
| `id` invalid | `400` |
| `id` sudah ada | `409` (`"node {id} sudah ada"`) |
| `valve_*` tidak valid | `400` |

### 10. `PUT /api/nodes/:device_id/config`

Body: `{ zone?: string, valve_1: object, valve_2: object }`

`valve_1` dan `valve_2` **wajib** ada (tipe `serde_json::Value` tanpa `#[serde(default)]`,
`routes.rs:341-342`). Validasi FIELDKSINTERNAL `ValveConfig::validate` (`src/config.rs:100`):

| Aturan | Rentang |
|--------|---------|
| `close_soil_min_pct` | 0..=100 |
| `close_temp_threshold_c` | -10..=80 |
| `max_open_duration_s` | 1..=3600 |
| `open_soil_max_pct` (mode sensor) | 0..=100 |
| `open_temp_threshold_c` (mode sensor) | -10..=80 |
| `open_temp_threshold_c >= close_temp_threshold_c` | histeresis suhu |
| `open_soil_max_pct <= close_soil_min_pct` | histeresis kelembapan |
| `schedule.len() <= 12` | |
| `start < end` | window tidak boleh terbalik |
| window tidak boleh melewati tengah malam | `start > end` ditolak |
| window tidak boleh tumpang tindih | setelah diurutkan by start |
| window tidak boleh duplikat persis | |

`zone` di-`trim`, string kosong → `null`, maksimum 64 karakter.
Field `enabled` **dipertahankan** dari nilai lama, tidak bisa diubah lewat endpoint ini
(`routes.rs:355-360`).

Respons: `{"ok": true, "node": {...}}`.

### 15. `POST /api/nodes/:device_id/command`

Body: `{ valve_id: 1|2, action: "open"|"close", duration_s: u32 }`

Validasi (`routes.rs:227-237`):

- node harus ada, selain itu `404`
- `valve_id` harus `1` atau `2`, selain itu `400`
- `action == "open"` mensyaratkan `duration_s > 0`, selain itu `400`

Perilaku penting: `published = state.mqtt.publish(...).is_ok()` (`routes.rs:252-256`).
State valve **hanya** di-update bila `published == true` (`routes.rs:277`).
Jadi bila broker MQTT mati, API tetap `200` dengan `{"published": false}` dan UI
tidak berubah — sanggat penting untuk setup pengujian.

Bila sukses:

1. Insert `decisions` dengan `reason = "manual_override"` (`routes.rs:265-276`).
2. `update_twin_valve_state` → `source = "manual"`, `expires_at = now + duration_s` bila open.
3. Cache in-memory twin di-update juga.

Respons: `{"command_id": "...", "published": true|false}`.

### 11. `POST /api/nodes/:device_id/deactivate`

Menolak dengan `400` bila ada valve yang masih terbuka (`routes.rs:432-438`):
`"{which} masih terbuka, tutup dulu sebelum menonaktifkan node"` di mana `which`
∈ {`valve 1`, `valve 2`, `valve 1 dan 2`}.

`404` bila node tidak ada. Sukses → `{"ok": true}`.

### 12. `POST /api/nodes/:device_id/activate`

Selalu sukses bila node ada. `404` bila tidak. → `{"ok": true}`.
Tidak ada pemeriksaan valve terbuka.

### 16. `POST /api/nodes/:device_id/valves/:valve_id/release`

Mengembalikan valve ke mode auto: hapus `source = manual` di DB + cache twin.
`404` bila node tidak ada; `400` bila `valve_id` bukan 1/2.
Respons: `{"ok": true}`.

### 8. `GET /api/schedule/active`

Hanya node dengan `enabled = true` yang diproses (`routes.rs:516`), dan hanya
`ValveConfig::Schedule` dengan `schedule` non-kosong (`routes.rs:518-519`).

Respons:

```json
{
  "now": "HH:MM",
  "now_epoch": 1234567890,
  "items": [{
    "device_id": "...", "zone": "...|null", "valve_id": 1,
    "mode": "schedule",
    "state": "running|upcoming",
    "window": {"start": "06:00", "end": "06:15"},
    "seconds_remaining": 123, "seconds_until_start": null, "tomorrow": false
  }]
}
```

`state = "running"` bila `now` berada di dalam window. Selain itu `seconds_remaining`
diisi dan `seconds_until_start`/`tomorrow` bernilai `null`. Untuk window berikutnya,
`seconds_remaining = null`, `seconds_until_start` diisi, `tomorrow = true` bila
`start <= now` (maka sudah lewat untuk hari ini → besok).

Array `items` bisa kosong → frontend menampilkan
"Mode sensor aktif (tidak ada jadwal waktu khusus)."

## Topik MQTT (bukan HTTP, tapi relevan untuk state)

Dari `src/mqtt/topics.rs`:

| Topik | Arah | Isi |
|-------|------|-----|
| `irigasi/ipesg/v1/+/sensor` | subscribe (wildcard) | JSON `SensorReading` |
| `irigasi/ipesg/v1/<device_id>/command` | publish | JSON `ValveCommand` |

`device_id_from_sensor_topic` hanya menerima topic dengan **tepat 5 segmen**
(`irigasi/ipesg/<version>/<device_id>/sensor`). Topik `.../command` tidak pernah
dianggap sensor.

## Error Shape

Semua error API konsisten (`routes.rs:63-72`):

```json
{ "error": "<pesan dalam bahasa Indonesia>" }
```

| Status | Dipakai untuk |
|--------|----------------|
| `400` | validasi input gagal |
| `401` | sesi tidak valid (middleware), kredensial login salah |
| `404` | node tidak ditemukan |
| `409` | konflik (node duplikat) |
| `500` | error internal |

## Catatan untuk Setup Pengujian

`config/default.toml` berisi kredensial **nyata** untuk HiveMQ Cloud
(`broker_host = "36d454...s1.eu.hivemq.cloud"`, TLS 8883). Menjalankan pengujian
lokal terhadap config itu akan:

- terhubung ke broker remote milik user,
- dan `POST /api/nodes/:id/command` akan **mempublikasikan perintah valve sungguhan**.

Karena itu harness E2E memakai config terpisah (`tests/e2e/config.e2e.toml`) yang
menunjuk ke broker mosquitto lokal dan path SQLite sementara.