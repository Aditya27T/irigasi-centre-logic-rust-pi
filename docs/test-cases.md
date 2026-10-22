# Test Cases — Fungsional & Smoke Dashboard `irigasi-core`

> **Status dokumen: DIHASILKAN DARI KODE, bukan spesifikasi awal.**
> Sumber: `docs/routes.md`, `docs/ui-components.md`, `src/dashboard/*`, `static/index.html`,
> `src/config.rs`.
>
> Kolom "Box" merujuk nomor bounding box di `docs/ui-components.md`, dipakai laporan
> screenshot akhir untuk menyatakan status validasi tiap elemen.

## Prasyarat Harness

Tidak ada test E2E di repo ini sebelumnya (`tests/` kosong). Harness dibuat di
`tests/e2e/` dengan prasyarat berikut:

| Prasyarat | Alasan |
|-----------|--------|
| `cargo build --features dashboard` | Dashboard di balik feature Cargo `dashboard` (`Cargo.toml:17`) |
| Broker MQTT lokal (mosquitto) di `127.0.0.1:1883` | Tanpa broker, `publish()` selalu `false` dan valve tidak pernah masuk state `manual` (`src/dashboard/routes.rs:252-256`) |
| Config khusus E2E: `tests/e2e/config.e2e.toml` | `config/default.toml` menunjuk HiveMQ Cloud **nyata** + berisi kredensial; `POST .../command` akan mengirim perintah ke perangkat asli |
| SQLite sementara (`data/e2e.db`, sudah tercakup `.gitignore`) | Isolasi data dari `data/irigasi.db` |
| Cookie sesi | Semua test kecuali `TC-001` butuh login |

Data sintetis disuntik lewat MQTT ke topic `irigasi/ipesg/v1/<device_id>/sensor` dengan
payload `SensorReading` (`src/lib.rs:86`), memakai `mosquitto_pub`.

**Dua senario eksekusi valve yang berbeda**, dan keduanya wajib diuji:

- `decision.dry_run = false` — engine benar-benar publish command ke broker.
- Decision engine membaca sensor dengan moisture rendah sehingga membuka valve otomatis
  → state `source = 'auto'`, `expires_at` terisi.

---

## Grup A — Autentikasi & Sesi

### TC-001 · Redirect dashboard tanpa sesi → login
**Box:** — (state pra-kondisi)

| | |
|---|---|
| Precondition | Request tanpa cookie `irigasi_session` |
| Steps | `GET /` tanpa cookie |
| Expected | HTTP `303 See Other` dengan `Location: /login`. Body dashboard **tidak** ter-leak |
| Ref | `src/dashboard/auth.rs:99-100` |

> Statusnya **303**, bukan 302: `axum::response::Redirect::to()` memakai default
> `StatusCode::SEE_OTHER` (`kind: RedirectKind::SeeOther`). Assertion harus 303.

### TC-002 · API endpoint menolak tanpa sesi dengan 401 JSON
**Box:** —

| | |
|---|---|
| Precondition | Tanpa cookie sesi |
| Steps | `GET /api/nodes` tanpa cookie |
| Expected | HTTP `401`, body `{"error":"sesi login tidak valid, silakan login ulang"}` — JSON, **bukan** HTML |
| Ref | `src/dashboard/auth.rs:101-106` |

> Menguji bentuk respons, bukan hanya status: frontend melakukan
> `res.json()` dan perbedaan ini akan muncul sebagai `Unexpected token '<'`
> (`static/index.html:2060`).

### TC-003 · Login gagal menampilkan banner error
**Box:** 4, 5, 6, 7 (`01-login-error`)

| | |
|---|---|
| Precondition | Di `/login` |
| Steps | Isi username `admin` / password `salah`, submit form |
| Expected | `#err` visible berisi "username atau password salah"; `#submit-btn` kembali enabled dengan teks "Masuk"; URL tetap `/login` |
| Ref | `static/login.html:91-98` |

### TC-004 · Login berhasil memberi sesi dan membuka dashboard
**Box:** 5, 6, 7 → seluruh peta `03-control`

| | |
|---|---|
| Precondition | Di `/login` |
| Steps | Isi `admin` / `admin123`, submit |
| Expected | Navigasi ke `/`; cookie `irigasi_session` terpasang dengan `HttpOnly`, `SameSite=Lax`, `Path=/`, `Max-Age=86400`; `#page-control` terlihat |
| Ref | `src/dashboard/auth.rs:31-47`, `static/login.html:88` |

### TC-005 · Tombol Keluar menghapus sesi
**Box:** 5 (`03-control`)

| | |
|---|---|
| Precondition | Sudah login di `/` |
| Steps | Klik `#logout-btn`, **terima** dialog konfirmasi |
| Expected | `POST /api/logout`; redirect ke `/login`; `GET /api/nodes` berikutnya `401` |
| Ref | `static/index.html:2550-2557`, `src/dashboard/auth.rs:49-59` |

> Memakai `page.on('dialog', d => d.accept())` — `confirm()` native memblokir eksekusi
> script bila tidak ditangani.

---

## Grup B — Halaman Kontrol & Pantau

### TC-006 · Empat KPI ter-render dengan nilai dari API
**Box:** 6, 7, 8, 9, 10 (`03-control`)

| | |
|---|---|
| Precondition | Login; 2 node aktif, keduanya online; 1 valve terbuka; 1 zona kering |
| Steps | Buka `/`, tunggu `#kpis` selesai render |
| Expected | Tepat 4 `.kpi-card`; KPI 1 `2 / 2`; KPI 2 `1 / 4`; KPI 3 ≥ 1; KPI 4 menampilkan cuaca atau state "Offline" yang sah |
| Ref | `static/index.html:1697` |

> KPI 4 bergantung internet (`api.open-meteo.com`). Kedua outcome — data atau
> "Offline" — valid; test hanya boleh gagal bila kartu tidak ada sama sekali.

### TC-007 · Kartu node menampilkan zona, ID, dan status online
**Box:** 14, 15, 16, 17, 18, 19 (`03-control`)

| | |
|---|---|
| Precondition | 2 node aktif; keduanya mengirim reading < 30 detik lalu |
| Steps | Buka `/` |
| Expected | 2 `.node-card`; tiap kartu punya `.node-device-id` = id node, `.status-pill.online` dengan `.live-dot`, dan `.node-last-seen` berisi tanggal/waktu (bukan "-") |
| Ref | `static/index.html:1943`, `src/dashboard/routes.rs:123` |

### TC-008 · Node tanpa reading tampil Offline dan gauge kosong
**Box:** 18, 20, 21 (`03-control`)

| | |
|---|---|
| Precondition | Tambahkan node baru lewat API **tanpa** pernah mengirim reading |
| Steps | Buka `/` |
| Expected | `.status-pill.offline`; kedua `.soil-gauge-item` menampilkan "Tidak ada sensor" dan badge "-"; `.progress-fill` lebar 0%; `.node-last-seen` = "-" |
| Ref | `src/dashboard/routes.rs:136-152`, `static/index.html:1797-1800`, `static/index.html:1827` |

### TC-009 · Gauge kelembapan memakai ambang mode yang benar
**Box:** 20, 21 (`03-control`)

| | |
|---|---|
| Precondition | Node A `mode = sensor` dengan `open_soil_max_pct = 35`; Node B `mode = schedule` dengan `close_soil_min_pct = 45`. Keduanya moisture 30% |
| Steps | Buka `/` |
| Expected | Gauge Node A: "Kering (Perlu Air)", bar amber, `.threshold-hint` "Ambang trigger: 35%" |
| | Gauge Node B: "Belum Cukup Basah", bar amber, `.threshold-hint` "Ambang trigger: 45%" |
| Ref | `static/index.html:1801-1824` |

> Ini test paling berharga di grup B: kedua mode memakai **arah ambang berlawanan**
> (`<` untuk sensor, `>=` untuk schedule). Tukar-balik di sini tidak akan menggagalkan
> build maupun typecheck — hanya test ini yang menangkapnya.

### TC-010 · Baris jadwal menampilkan state running dan upcoming
**Box:** 11, 12 (`03-control`)

| | |
|---|---|
| Precondition | Satu node dengan window yang sedang berjalan; satu node dengan window berikutnya |
| Steps | Buka `/` |
| Expected | `.schedule-item.is-running` memuat "Mengalir" + sisa waktu format `Xj Ym Zd`; `.schedule-item` (tanpa `is-running`) memuat "dlm" + "(besok)" bila besok |
| Ref | `static/index.html:1763-1785`, `static/index.html:1545-1549` |

### TC-011 · Memilih chip durasi tidak mengirim request
**Box:** 29 (`03-control`)

| | |
|---|---|
| Precondition | Di `/`; valve 1 tertutup |
| Steps | Klik chip "5 mnt" pada valve 1 |
| Expected | Hanya chip itu yang jadi `.is-selected`; **tidak ada** request ke `/api/nodes/*/command`; badge badge valve tetap "Otomasi Aktif" |
| Ref | `static/index.html:2000-2005` |

### TC-012 · Membuka valve secara manual membuat badge Override + tombol Auto
**Box:** 24, 27, 28, 30, 32 (`04-valve-manual`)

| | |
|---|---|
| Precondition | Broker MQTT **terhubung**; valve 1 tertutup, `source = auto` |
| Steps | Pilih chip 2 mnt, klik `.valve-card[data-valve="1"] .btn-toggle-switch` |
| Expected | `POST /api/nodes/<id>/command` body `{valve_id:1, action:"open", duration_s:120}`; toast sukses; `.valve-card` dapat `.is-open`; badge berubah ke "⚠️ Override Manual"; **tombol "Kembalikan ke Auto" muncul**; `.valve-subtext` berisi "Mengalir · Berhenti dlm" |
| Ref | `static/index.html:2024-2028`, `static/index.html:2038`, `src/dashboard/routes.rs:277-297` |

> **Syarat MQTT tidak bisa dilewati.** Bila broker mati, `published: false`, dan
> `routes.rs:277`/skips seluruh update state — badge tetap "Otomasi Aktif" dan tombol
> release tidak pernah muncul, walaupun respons API tetap `200`. Kegagalan TC-012
> berarti environment, bukan UI.

### TC-013 · Menutup valve manual menahan override; Kembalikan ke Auto melepas
**Box:** 24, 27, 28, 32 (`04-valve-manual` → closed, lalu released)

| | |
|---|---|
| Precondition | Valve 1 tertutup di mode auto (di-release dulu lewat API) |
| Steps | Buka manual via `.btn-toggle-switch`, tutup lagi, lalu klik "Kembalikan ke Auto" |
| Expected | Request `action: "close"`; valve tertutup; badge TETAP "⚠️ Override Manual" dan tombol "Kembalikan ke Auto" masih ada (source tetap `manual`); setelah release badge kembali "⚡ Otomasi Aktif", tombol hilang; `.valve-subtext` = "Siaga (Tertutup)" |
| Ref | `static/index.html:1891`, `static/index.html:1856-1886`, `src/twin/mod.rs:116` |

### TC-014 · "Kembalikan ke Auto" menghapus override manual
**Box:** 30, 32 (`04-valve-manual` → released)

| | |
|---|---|
| Precondition | Valve 2 terbuka dengan `source = manual` |
| Steps | Klik `.btn-release-auto` pada valve 2 |
| Expected | `POST /api/nodes/<id>/valves/2/release`; badge "⚡ Otomasi Aktif"; tombol release hilang; toast "Valve 2 dikembalikan ke mode Otomatis" |
| Ref | `static/index.html:2021-2022`, `static/index.html:2039-2041`, `src/dashboard/routes.rs:311-335` |

### TC-015 · Navigasi tab hanya mengganti state, bukan URL
**Box:** 3 (`03-control` → `05-data` → `06-settings`)

| | |
|---|---|
| Precondition | Login di `/` |
| Steps | Klik tab Data & Analitik, lalu Pengaturan, lalu Kontrol & Pantau |
| Expected | URL tetap `/` sepanjang proses; tepat satu section tanpa `hidden` pada setiap langkah; `.nav-tabs button.is-active` bergeser sesuai tab |
| Ref | `static/index.html:1568-1575` |

---

## Grup C — Data & Analitik

### TC-016 · Halaman data memuat readings dan decisions lalu menggabungkannya
**Box:** 1, 2, 3, 4, 5, 6, 7 (`05-data`)

| | |
|---|---|
| Precondition | Readings dan ≥ 1 keputusan `action != none` untuk 2 node |
| Steps | Klik tab Data & Analitik |
| Expected | `#filter-node` memuat chip "Semua" + 2 id node; `#data-rows` punya bariscampuran "Pembacaan" dan "Keputusan"; `#data-stats` Total ≥ jumlah baris yang ditampilkan |
| Ref | `static/index.html:2113-2142`, `static/index.html:2150-2155` |

> Request per node: `/readings?limit=300` dan `/decisions?limit=300`, lalu `.slice(0, 200)`
> untuk render (`index.html:2121`, `index.html:2157`).

### TC-017 · Filter node dan jenis log mengubah isi tabel
**Box:** 2, 3, 5, 7 (`05-data`)

| | |
|---|---|
| Precondition | 2 node, keduanya punya data |
| Steps | Pilih chip node kedua; amati jumlah baris. Lalu pilih chip "Keputusan"; amati jumlah baris |
| Expected | Hanya baris node terpilih tersisa; lalu hanya `kind === 'Keputusan'` tersisa; `#data-stats` "Total Baris Rekaman" ikut berubah; chip aktif bergeser |
| Ref | `static/index.html:2089-2103`, `static/index.html:2146` |

### TC-018 · Filter rentang waktu 6 jam menyaring data lama
**Box:** 4, 5, 7 (`05-data`)

| | |
|---|---|
| Precondition | Readings dengan `received_at` tersebar: sebagian < 6 jam, sebagian > 24 jam |
| Steps | Bandingkan jumlah baris pada rentang "24 Jam" vs "6 Jam Terakhir" |
| Expected | Jumlah baris rentang 6 jam ≤ rentang 24 jam, dan keduanya > 0 |
| Ref | `static/index.html:2083-2086`, `static/index.html:2145` |

### TC-019 · Unduh CSV menghasilkan file dengan baris terfilter
**Box:** 8 (`05-data`)

| | |
|---|---|
| Precondition | Filter node = satu node, rentang 24 jam |
| Steps | Klik `#download-csv`, tangkap event download |
| Expected | Nama file `irigasi_<nodeId>_24jam.csv`; isi punya header `waktu,node,jenis,...` (8 kolom) + BOM `\ufeff`; **jumlah baris data = "Total Baris Rekaman" yang tampil**, bukan seluruh tabel |
| Ref | `static/index.html:2184-2206`, `static/index.html:2181` |

> Handler di-assign ulang tiap render (`.onclick`), jadi test harus listens download
> *setelah* tabel selesai ter-filter.

---

## Grup D — Pengaturan

### TC-020 · Kartu konfigurasi memuat zone, status, dan dua fieldset valve
**Box:** 4, 5, 6, 7, 8, 10, 19 (`06-settings`)

| | |
|---|---|
| Precondition | 2 node |
| Steps | Klik tab Pengaturan |
| Expected | 2 `.settings-card`; tiap punya 2 `fieldset[data-valve]`; `input[data-field="zone"]` terisi sesuai config; pill "Node Aktif" |
| Ref | `static/index.html:2238-2261` |

### TC-021 · Beralih mode kontrol sensor ↔ jadwal tanpa request
**Box:** 11, 12, 13 (`06-settings`)

| | |
|---|---|
| Precondition | Valve 1 dalam mode schedule |
| Steps | Klik chip "Sensor Ambang (Otomatis)" pada valve 1 |
| Expected | `.mode-sensor` visible, `.mode-schedule` hidden; chip aktif bergeser; **tidak ada** request |
| Ref | `static/index.html:2363-2370` |

### TC-022 · Menyimpan konfigurasi valid_round-trip ke server
**Box:** 17, 32 (`06-settings`)

| | |
|---|---|
| Precondition | Ubah `input[data-field="zone"]` menjadi "Zona Uji", ubah `close_soil_min_pct` valve 1 menjadi 50 |
| Steps | Klik "Simpan Pengaturan" |
| Expected | `PUT /api/nodes/<id>/config` dengan `zone = "Zona Uji"`; toast sukses; `.form-error` tetap hidden; kartu ter-re-render dengan nilai baru; `GET /api/nodes/<id>/config` mengembalikan nilai yang sama |
| Ref | `static/index.html:2387-2418`, `src/dashboard/routes.rs:345-381` |

### TC-023 · Validasi client menolak window jadwal yang tidak valid
**Box:** 16 (`06-settings`)

| | |
|---|---|
| Precondition | Valve 1 mode schedule, hapus semua window lalu tambah 2 window |
| Steps | Set window 1 `10:00`–`12:00`, window 2 `11:00`–`13:00` (tumpang tindih), klik Simpan |
| Expected | `.form-error` visible dengan teks berawalan **"Validasi: "**; **tidak ada** request `PUT`; nilai form tidak berubah |
| Ref | `static/index.html:2463-2490`, `static/index.html:2394` |

### TC-024 · Validasi client menolak window melewati tengah malam
**Box:** 16 (`06-settings`)

| | |
|---|---|
| Precondition | Mode schedule aktif |
| Steps | Window `22:00`–`02:00`, klik Simpan |
| Expected | `.form-error` memuat "Validasi: " + "melewati tengah malam"; tidak ada request |
| Ref | `static/index.html:2472-2474`, `src/config.rs:158-161` |

### TC-025 · "Batal Perubahan" mengembalikan nilai server
**Box:** 18 (`06-settings`)

| | |
|---|---|
| Precondition | Ubah zone menjadi "Salah" **tanpa** menyimpan |
| Steps | Klik "Batal Perubahan" |
| Expected | `input[data-field="zone"]` kembali ke nilai server; toast info "Perubahan dibatalkan"; kartu ter-re-render dari `settingsState` |
| Ref | `static/index.html:2381-2386` |

### TC-026 · Nonaktifkan node menghilangkan dari halaman Kontrol
**Box:** 9, 8 (`08-node-disabled`)

| | |
|---|---|
| Precondition | Kedua valve node tersebut tertutup |
| Steps | Di Pengaturan, klik "Nonaktifkan" pada node A |
| Expected | `POST /api/nodes/<id>/deactivate`; pill menjadi "Dinonaktifkan"; tombol berganti "Aktifkan"; di tab Kontrol hanya tersisa 1 `.node-card`; node A **tetap** muncul di chip filter Data & Analitik |
| Ref | `static/index.html:2425-2436`, `static/index.html:2239-2245`, `static/index.html:2114` |

### TC-027 · Nonaktifkan ditolak bila valve masih terbuka
**Box:** 16, 32 (`06-settings`)

| | |
|---|---|
| Precondition | Valve 1 node A terbuka manual |
| Steps | Klik "Nonaktifkan" pada node A |
| Expected | `POST` mengembalikan `400`; toast error memuat "masih terbuka, tutup dulu sebelum menonaktifkan node"; status node **tidak** berubah |
| Ref | `src/dashboard/routes.rs:432-438`, `static/index.html:2428-2431` |

> Interaksi ini yang membuat TC-012 (butuh broker MQTT tersambung) menjadi prasyarat,
> bukan sekadar kosmetik.

### TC-028 · Aktifkan node mengembalikannya ke halaman Kontrol
**Box:** 9 (`08-node-disabled` → `06-settings`)

| | |
|---|---|
| Precondition | Node A nonaktif (dari TC-026) |
| Steps | Klik "Aktifkan" pada node A |
| Expected | `POST /api/nodes/<id>/activate`; pill "Node Aktif"; grid Kontrol kembali 2 kartu |
| Ref | `src/dashboard/routes.rs:459-477` |

### TC-029 · Modal tambah node menolak ID kosong
**Box:** 8, 10 (`07-modal-add-node`)

| | |
|---|---|
| Precondition | Di Pengaturan |
| Steps | Klik "Tambah Node Baru", klik "Simpan Node" tanpa mengisi ID |
| Expected | `#modal-add-node-err` visible berisi "ID Node wajib diisi!"; modal tetap terbuka; tidak ada request |
| Ref | `static/index.html:2516-2520` |

### TC-030 · Modal menolak ID dengan format terlarang
**Box:** 8 (`07-modal-add-node`)

| | |
|---|---|
| Precondition | Modal terbuka |
| Steps | Isi ID `node/03`, klik "Simpan Node" |
| Expected | Modal tetap terbuka; `#modal-add-node-err` berisi pesan `400` dari server ("id tidak valid (A-Z a-z 0-9 _ - , max 64)") |
| Ref | `static/index.html:2529-2531`, `src/dashboard/routes.rs:397-399` |

### TC-031 · Modal menolak node duplikat
**Box:** 8 (`07-modal-add-node`)

| | |
|---|---|
| Precondition | Node `esp32-01` sudah ada |
| Steps | Isi ID `esp32-01`, klik "Simpan Node" |
| Expected | Modal tetap terbuka; error memuat "sudah ada" (status `409`) |
| Ref | `src/dashboard/routes.rs:402-404` |

### TC-032 · Modal membuat node baru yang langsung tampil
**Box:** 2, 5, 10, 32 (`07-modal-add-node` → `06-settings`)

| | |
|---|---|
| Precondition | Modal terbuka |
| Steps | Isi ID `node-uji-99` dan zone `Lab Uji`, klik "Simpan Node" |
| Expected | Modal tertutup; toast sukses memuat id node; kartu baru muncul di `#settings-nodes` dengan zone "Lab Uji"; kartu baru juga muncul di `#nodes` |
| Ref | `static/index.html:2533-2537`, `src/dashboard/routes.rs:427` |

### TC-033 · "Muat Ulang" mengambil ulang konfigurasi dari server
**Box:** 3 (`06-settings`)

| | |
|---|---|
| Precondition | Di Pengaturan |
| Steps | Klik "Muat Ulang" |
| Expected | `GET /api/settings`; toast info "Pengaturan dimuat ulang"; isi kartu tidak berubah |
| Ref | `static/index.html:2544-2547` |

---

## Grup E — Keahanan & Non-Regresi

### TC-034 · Polling 4 detik tidak menumpuk atau merusak DOM
**Box:** 6, 14, 15 (`03-control`)

| | |
|---|---|
| Precondition | Login di `/` |
| Steps | Tunggu 12 detik tanpa berinteraksi |
| Expected | `#nodes` berisi tepat 2 `.node-card` (bukan 4 atau 0); tidak ada error di console; `#kpis` berisi tepat 4 kartu |
| Ref | `static/index.html:2560-2561` |

### TC-035 · Error handler frontend tidak melempar exception
**Box:** —

| | |
|---|---|
| Precondition | Login, dashboard terbuka |
| Steps | Kumpulkan `pageerror` selama 20 detik browsing ketiga tab |
| Expected | Nol `pageerror` |
| Ref | menyeluruh |

> Polling 4 detik + `loadWeather` yang memanggil API luar membuat ini worthwhile:
> kegagalan fetch cuaca sengaja di-`catch` (`index.html:1591-1600`) dan tidak boleh
> memunculkan unhandled rejection.

### TC-036 · Viewport 1920x1080 tidak memunculkan layout rusak
**Box:** seluruh peta

| | |
|---|---|
| Precondition | Login |
| Steps | Set viewport persis 1920×1080, buka ketiga tab |
| Expected | Tidak ada scroll horizontal (`documentElement.scrollWidth <= 1920`); grid KPI tetap 4 kolom; grid node tidak terpotong |
| Ref | `static/index.html` media query |

### TC-037 · Race pada klik cepat valve tidak menghasilkan state korup
**Box:** 27, 32 (`04-valve-manual`)

| | |
|---|---|
| Precondition | Valve 1 tertutup |
| Steps | Klik `.btn-toggle-switch` valve 1 tiga kali cepat berurutan |
| Expected | Tombol `disabled` selama request (`index.html:2019`), sehingga hanya 1 request terkirim; state akhir valve konsisten dengan respons server (terbuka, manual) |
| Ref | `static/index.html:2018-2049` |

---

## Matriks Box → Test

| Box | Deskripsi singkat | Test |
|-----|-------------------|------|
| L1–L7 | Halaman login & banner error | TC-003, TC-004 |
| C1 | Topbar | TC-005, TC-015 |
| C2 | Brand group | — |
| C3 | Navigasi tab | TC-015 |
| C4 | Jam lokal | — (kosmetik, tick 1 detik) |
| C5 | Tombol Keluar | TC-005 |
| C6 | Grid KPI | TC-006, TC-034 |
| C7 | KPI Status Jaringan | TC-006 |
| C8 | KPI Katup Aktif | TC-006 |
| C9 | KPI Kondisi Lahan | TC-006 |
| C10 | KPI Cuaca | TC-006 (tersedia dua state sah) |
| C11 | Widget Jadwal | TC-010 |
| C12 | Baris jadwal | TC-010 |
| C13 | Section bar | — (kosmetik) |
| C14 | Grid node | TC-007, TC-034 |
| C15 | Kartu node | TC-007, TC-034 |
| C16 | Tag zona | TC-007 |
| C17 | ID node | TC-007 |
| C18 | Pill online/offline | TC-007, TC-008 |
| C19 | Waktu update | TC-007, TC-008 |
| C20 | Gauge moisture 1 | TC-008, TC-009 |
| C21 | Gauge moisture 2 | TC-008, TC-009 |
| C22 | Tile suhu 1 | — (kosmetik, tidak ada aksi) |
| C23 | Tile suhu 2 | — (kosmetik, tidak ada aksi) |
| C24 | Kartu valve 1 | TC-012, TC-013, TC-037 |
| C25 | Judul valve + badge AKTIF | TC-012 |
| C26 | Status aliran | TC-012, TC-013 |
| C27 | Switch buka/tutup | TC-012, TC-013, TC-037 |
| C28 | Badge mode | TC-012, TC-013, TC-014 |
| C29 | Chip durasi | TC-011 |
| C30 | Tombol kembali ke auto | TC-014 |
| C31 | Kartu valve 2 | TC-014 |
| C32 | Wadah toast | TC-012, TC-013, TC-014, TC-022, TC-027, TC-032 |
| D1 | Kartu filter | TC-016 |
| D2 | Chip filter node | TC-016, TC-017, TC-026 |
| D3 | Chip filter jenis | TC-017 |
| D4 | Chip rentang waktu | TC-018 |
| D5 | Bar statistik | TC-016, TC-017 |
| D6 | Header tabel | TC-016 |
| D7 | Badan tabel | TC-016, TC-017, TC-018 |
| D8 | Tombol unduh CSV | TC-019 |
| S1 | Toolbar pengaturan | — (kosmetik) |
| S2 | Tombol tambah node | TC-029, TC-032 |
| S3 | Tombol muat ulang | TC-033 |
| S4 | Wadah kartu konfigurasi | TC-020 |
| S5 | Kartu konfigurasi | TC-020, TC-032 |
| S6 | Judul node | TC-020 |
| S7 | Input zona | TC-022, TC-025 |
| S8 | Pill status konfigurasi | TC-020, TC-026 |
| S9 | Tombol aktif/nonaktif | TC-026, TC-027, TC-028 |
| S10 | Fieldset valve 1 | TC-020 |
| S11 | Mode switch | TC-021 |
| S12 | Grup input sensor | TC-021 |
| S13 | Grup input jadwal | TC-021 |
| S14 | Baris window | TC-023, TC-024 |
| S15 | Tombol tambah window | TC-023 |
| S16 | Banner error validasi | TC-023, TC-024, TC-027 |
| S17 | Tombol simpan | TC-022, TC-023, TC-024 |
| S18 | Tombol batal | TC-025 |
| S19 | Fieldset valve 2 | TC-020 |
| M1–M10 | Modal tambah node | TC-029, TC-030, TC-031, TC-032 |

Box bertanda "—" adalah elemen murni kosmetik tanpa interaksi maupun sumber data
(heading, ikon, label). Elemen tersebut tetap difoto dan didokumentasikan, tetapi tidak
ada test fungsional yang mengikatnya — dan laporan final menyatakan itu secara eksplisit
alih-alih mengklaim "tervalidasi".