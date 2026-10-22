# Komponen UI — Dashboard `irigasi-core`

> **Status dokumen: DIHASILKAN DARI KODE, bukan spesifikasi awal.**
> Sumber: `static/index.html` (2566 baris, CSS inline + JS vanilla), `static/login.html`.
> Dokumen ini adalah sumber kebenaran untuk **nomor bounding box** pada screenshot anotasi.

## Arsitektur Layar

Dashboard adalah **SPA satu halaman tanpa router**: `GET /` selalu mengembalikan HTML yang
sama. Navigasi antar tab hanya lewat `switchPage()` (`static/index.html:1568`) yang
meng-toggle atribut `hidden` pada tiga `<section>`:

| Section | ID | Ditampilkan saat |
|---------|----|------------------|
| Kontrol & Pantau | `#page-control` | `page === 'control'` (default, tidak punya `hidden`) |
| Data & Analitik | `#page-data` | `page === 'data'` |
| Pengaturan | `#page-settings` | `page === 'settings'` |

Konsekuensi untuk pengujian: **hanya ada 2 URL HTML** (`/login` dan `/`). Semua "halaman"
lain adalah state klien pada URL yang sama. Screenshot harus merekam state tab, bukan URL.

Polling otomatis (`static/index.html:2559-2563`):

| Timer | Interval | Fungsi |
|-------|----------|--------|
| `loadNodes()` | 4000 ms | refetch `/api/nodes` + render KPI, kartu node, jadwal |
| `tickClock()` | 1000 ms | jam di topbar + re-render kartu jadwal |
| `loadWeather()` | 15 menit | Open-Meteo (lat -7.9666, lon 112.6326 — Malang) |

`loadWeather()` memanggil API pihak ketiga `api.open-meteo.com`. Pada kondisi offline
KPI cuaca jatuh ke kartu "Offline / Gagal memuat Open-Meteo" (`index.html:1709-1717`) —
itu state valid, bukan bug.

## Peta State Screenshot

| ID Screenshot | Cara mencapai state |
|---------------|---------------------|
| `01-login` | `GET /login` tanpa sesi |
| `02-login-error` | `01` + submit kredensial salah |
| `03-control` | login sukses → `/` (tab default `control`) |
| `04-valve-manual` | `03` + buka valve 1 secara manual (state `manual`) |
| `05-data` | `03` + klik tab "Data & Analitik" |
| `06-settings` | `03` + klik tab "Pengaturan" |
| `07-modal-add-node` | `06` + klik "Tambah Node Baru" |
| `08-node-disabled` | `06` + nonaktifkan sebuah node |

---

# Peta Bounding Box

Nomor **dimulai ulang dari 1 pada setiap screenshot** — ini konvensi standar laporan
anotasi, dan scripted overlay di `tests/e2e/annotate.mjs` mengikuti konvensi yang sama.

---

## `01-login` / `02-login-error` — Halaman Masuk

Laman: `static/login.html`. Root `.card` max-width 360px, flex-center vertikal.

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 1 | `.card` | Kartu login | Wadah seluruh form, elevasi + border | `login.html:28` |
| 2 | `.card h1` | Judul "Irigasi Dashboard" | Identitas aplikasi | `login.html:55` |
| 3 | `.card p.sub` | Subtitle | Instruksi singkat "Masuk untuk melihat…" | `login.html:56` |
| 4 | `#err` | Banner error | Menampilkan `body.error` dari `/api/login`; `hidden` default | `login.html:57` |
| 5 | `label[for="u"]` + `#u` | Input Username | `autocomplete="username"`, `autofocus` | `login.html:59-60` |
| 6 | `label[for="p"]` + `#p` | Input Password | `type=password`, `autocomplete="current-password"` | `login.html:61-62` |
| 7 | `#submit-btn` | Tombol "Masuk" | Submit form; saat loading: disabled + teks "Memeriksa…" | `login.html:63`, `login.html:76` |

**Perilaku yang harus divalidasi:**

- Submit wrong credentials → `#err` jadi terlihat dengan teks
  "username atau password salah"; tombol kembali enabled dan teksnya "Masuk"
  (`login.html:91-98`). Form **tidak** pindah halaman.
- Submit benar → `location.href = '/'` (`login.html:88`).

---

## `03-control` — Kontrol & Pantau (tab default)

### Area: Topbar

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 1 | `header.topbar` | Topbar | Bar atas sticky: brand + nav + jam + logout | `index.html:1281` |
| 2 | `.brand-group` | Brand group | Logo SVG + judul + tagline "Telemetri & Kontrol Presisi Real-time" | `index.html:1282` |
| 3 | `nav.nav-tabs` | Navigasi tab | 3 tombol: Kontrol & Pantau / Data & Analitik / Pengaturan | `index.html:1301` |
| 4 | `.clock-pill` | Jam lokal | Tanggal + waktu lokal `id-ID`, tick tiap 1 detik | `index.html:1317`, `index.html:1584` |
| 5 | `#logout-btn` | Tombol Keluar | `confirm()` native → `POST /api/logout` → redirect `/login` | `index.html:1321`, `index.html:2550` |

### Area: KPI

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 6 | `#kpis` | Grid KPI | 4 kartu, re-render penuh tiap 4 detik | `index.html:1331`, `index.html:1697` |
| 7 | `#kpis .kpi-card:nth-child(1)` | KPI Status Jaringan Node | `online / total`; sub-teks hijau "Semua node normal" atau amber "Sebagian node offline". Amber bila ada valve terbuka | `index.html:1643` |
| 8 | `#kpis .kpi-card:nth-child(2)` | KPI Katup / Valve Aktif | `terbuka / total valve`; kartu diberi kelas `blue` bila >0 | `index.html:1661` |
| 9 | `#kpis .kpi-card:nth-child(3)` | KPI Kondisi Lahan | Jumlah zona kering/berjadwal; kelas `amber` bila >0 | `index.html:1679` |
| 10 | `#kpis .kpi-card:nth-child(4)` | KPI Cuaca · Malang | Open-Meteo: ikon, suhu, kelembapan, penanda hujan | `index.html:1700` |

Logika "Kondisi Lahan" (penting untuk test — `index.html:1630-1640`): sebuah node dihitung
kering bila **salah satu** dari: sensor moisture di bawah `open_soil_max_pct` (hanya mode
`sensor`), **atau** ada item jadwal `state === 'running'` untuk node tersebut.

### Area: Jadwal

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 11 | `#schedule-card` | Widget "Jadwal Siram" | Bar ringkas di atas grid node | `index.html:1336` |
| 12 | `#schedule-rows` | Baris jadwal | `running` → "Mengalir [win] · sisa X"; `upcoming` → "[win] dlm X (besok)". Kosong → "Mode sensor aktif (tidak ada jadwal waktu khusus)." | `index.html:1343`, `index.html:1752` |

### Area: Kartu Node

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 13 | `.section-bar` | Judul "Kontrol & Telemetri Zona" | Pembatas seksi + sub-teks "Status Terkini Sensor & Aktuator" | `index.html:1349` |
| 14 | `#nodes` | Grid kartu node | Satu `.node-card` per node aktif; grid kosong punya tombol "+ Daftarkan Node Pertama" | `index.html:1358`, `index.html:1928-1935` |
| 15 | `.node-card` | Kartu node | Wadah telemetry + valve untuk 1 node | `index.html:1948` |
| 16 | `.node-zone-tag` | Tag zona | `"ZONA: <zone>"`, atau `"ZONA UMUM"` bila `zone` null | `index.html:1951` |
| 17 | `.node-device-id` | ID node | `device_id` mentah | `index.html:1952` |
| 18 | `.node-card-head .status-pill` | Pill status online/offline | `.online` bila `last_reading_at` ≤ 30 detik lalu | `index.html:1943` |
| 19 | `.node-card-head .node-last-seen` | Waktu update | `fmtTime(last_reading_at)`, atau `-` bila null | `index.html:1956` |

### Area: Gauge Kelembapan (per node)

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 20 | `.soil-container .soil-gauge-item:nth-child(1)` | Gauge Soil Moisture 1 | Persentase + bar + ambang | `index.html:1962`, `index.html:1789` |
| 21 | `.soil-container .soil-gauge-item:nth-child(2)` | Gauge Soil Moisture 2 | Idem untuk sensor 2 | `index.html:1963` |

Tiga sub-part per gauge (bisa di-box bila perlu detail):

| Sub-selector | Isi | Sumber |
|--------------|-----|--------|
| `.soil-gauge-title` | Nama gauge + ikon tetes | `index.html:1833` |
| `.soil-val-badge` | Persentase + verdict ("Kering (Perlu Air)", "Kelembapan Cukup", "Cukup Basah", "Belum Cukup Basah", "Tidak ada sensor") | `index.html:1837` |
| `.progress-track` / `.progress-fill` | Bar; `width` = clamp(pct, 2, 100); warna amber bila perlu air | `index.html:1842` |
| `.threshold-hint` | `0%` · `Ambang trigger: N%` · `100%` | `index.html:1845` |

Perhatikan perbedaan arah ambang (`index.html:1801-1824`): mode **sensor** memakai
`open_soil_max_pct` (kering bila `pct < threshold`), mode **schedule** memakai
`close_soil_min_pct` (basah bila `pct >= threshold`). Uji harus memverifikasi kedua arah ini
tidak tertukar.

### Area: Metric Suhu (per node)

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 22 | `.metrics-tiles .metric-tile:nth-child(1)` | Tile Suhu Sensor 1 | `fmtNum(last_temp_1_c)` + "°C" | `index.html:1973` |
| 23 | `.metrics-tiles .metric-tile:nth-child(2)` | Tile Suhu Sensor 2 | `fmtNum(last_temp_2_c)` + "°C" | `index.html:1983` |

### Area: Kartu Valve (2 per node)

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 24 | `.valve-card[data-valve="1"]` | Kartu Valve 1 | Wadah kontrol actuator valve 1 | `index.html:1877` |
| 25 | `.valve-card[data-valve="1"] .valve-name-title` | Judul "Valve 1" | + badge "💧 AKTIF" bila terbuka | `index.html:1881` |
| 26 | `.valve-card[data-valve="1"] .valve-subtext` | Status aliran | `"Mengalir · Berhenti dlm M:SS"` atau `"Siaga (Tertutup)"` | `index.html:1884` |
| 27 | `.valve-card[data-valve="1"] .btn-toggle-switch` | Switch buka/tutup | `data-action = open\|close` sesuai state saat render | `index.html:1891` |
| 28 | `.valve-card[data-valve="1"] .valve-badge-row` | Badge mode | "Mode Jadwal Waktu" / "Mode Sensor Ambang" + "Override Manual" / "Otomasi Aktif" | `index.html:1895` |
| 29 | `.valve-card[data-valve="1"] .dur-picker-group` | Pemilih durasi | 3 chip: 60 dtk / 2 mnt / 5 mnt; default terpilih = 120 dtk (index 1) | `index.html:1906` |
| 30 | `.valve-card[data-valve="1"] .btn-release-auto` | "Kembalikan ke Auto" | **Hanya ada bila `source === 'manual'`** | `index.html:1914` |
| 31 | `.valve-card[data-valve="2"]` | Kartu Valve 2 | Serupa, untuk valve 2 | `index.html:1888` |

`data-device` dan `data-valve` pada `.valve-card` adalah sumber untuk endpoint yang dipanggil
(`index.html:2011-2013`). Delegasi event terpasang di `#nodes` (`index.html:1998`).

**Detail penting durasi:** chip durasi hanya mengubah tampilan (`class is-selected`,
`index.html:2003`) dan **tidak** mengirim request. Nilai dibaca saat tombol aksi diklik
(`index.html:2015-2016`), default 120 bila tidak ada chip terpilih.

### Area: Toast

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 32 | `#toasts` | Wadah toast | Ditambah `showToast()`, hilang otomatis 3500 ms + fade 300 ms | `index.html:1452`, `index.html:1502`, `index.html:1519` |

Jenis toast: `.toast.success` (hijau), `.toast.error` (merah), `.toast.info` (biru).

---

## `04-valve-manual` — State override manual

State yang sama dengan `03-control`, tetapi valve 1 sedang `source = 'manual'`.
Perbedaan yang harus terlihat:

| Selisih vs `03-control` | Element | Sumber |
|-------------------------|---------|--------|
| Badge **"⚠️ Override Manual"** (kelas `.manual-held`) menggantikan "⚡ Otomasi Aktif" | `.valve-badge-row` | `index.html:1899` |
| Tombol **"Kembalikan ke Auto"** menjadi ada | `.btn-release-auto` | `index.html:1914` |
| `.valve-card` dapat kelas `.is-open` | `.valve-card` | `index.html:1877` |
| `.valve-subtext` memakai kelas `.is-active` | `.valve-subtext` | `index.html:1884` |
| Toast sukses tampil | `.toast.success` | `index.html:2038` |

Kondisi valve manual hanya terbentuk bila MQTT **berhasil** publish
(`src/dashboard/routes.rs:277`). Bila broker mati, `published: false`, badge tetap
"Otomasi Aktif" dan tombol release tidak muncul — meski API mengembalikan `200`.
Ini harus diuji secara eksplisit.

---

## `05-data` — Data & Analitik

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 1 | `.filters-card` | Kartu filter | 3 grup filter chips | `index.html:1363` |
| 2 | `#filter-node` | Chip filter node | `Semua` + satu chip per `device_id` (termasuk node nonaktif) | `index.html:1366`, `index.html:2081` |
| 3 | `#filter-kind` | Chip filter jenis log | `Semua` / `Pembacaan` / `Keputusan` | `index.html:1370` |
| 4 | `#filter-range` | Chip rentang waktu | `6 Jam Terakhir` / `24 Jam` / `7 Hari`, default `24 Jam` | `index.html:1374`, `index.html:2083` |
| 5 | `#data-stats` | Bar statistik | Total baris / data sensor / aksi valve / rata-rata moisture 1 | `index.html:1378`, `index.html:2150` |
| 6 | `.table-container table thead` | Header tabel | 8 kolom: Waktu, Node, Jenis Rekaman, SM1, SM2, Suhu 1, Suhu 2, Keterangan | `index.html:1384` |
| 7 | `#data-rows` | Badan tabel | Maks **200** baris tampil (`.slice(0, 200)`); tabel kosong → 1 `<tr>` colspan 8 | `index.html:1396`, `index.html:2157`, `index.html:2173` |
| 8 | `#download-csv` | Tombol "Unduh Riwayat CSV" | Unduh hasil **terfilter**, bukan seluruh tabel; nama file `irigasi_<node>_<range>jam.csv`, BOM UTF-8 | `index.html:1400`, `index.html:2184` |

Baris `Keputusan` difilter `action !== 'none'` (`index.html:2131`), jadi keputusan
"tidak ada pemicu" tidak pernah muncul di tabel — meski tetap tersimpan di DB.
Kolom `note` berformat `Valve N · ACTION · <reason>` (`index.html:2135`).

Data diambil per node dengan `limit=300` untuk readings dan decisions
(`index.html:2121-2122`), lalu digabung client-side dan diurutkan `ts` menurun
(`index.html:2140`).

---

## `06-settings` — Pengaturan

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 1 | `.settings-toolbar` | Toolbar pengaturan | Judul + deskripsi | `index.html:1408` |
| 2 | `#btn-add-node` | "Tambah Node Baru" | Buka modal | `index.html:1414` |
| 3 | `#btn-reload-settings` | "Muat Ulang" | `GET /api/settings` ulang + toast info | `index.html:1418` |
| 4 | `#settings-nodes` | Wadah kartu konfigurasi | 1 `.settings-card` per node (aktif **dan** nonaktif) | `index.html:1424` |
| 5 | `.settings-card` | Kartu konfigurasi node | `data-device` = id node | `index.html:2248` |
| 6 | `.settings-device-title` | Judul node | Ikon chip SVG + `id` | `index.html:2250` |
| 7 | `input[data-field="zone"]` | Input zona | Persistensi `zone`; lebar tetap 180px | `index.html:2255` |
| 8 | `.settings-card-header .status-pill` | Pill status konfigurasi | "Node Aktif" / "Dinonaktifkan" | `index.html:2256` |
| 9 | `[data-act="activate"]` / `[data-act="deactivate"]` | Tombol aktif/nonaktif | Tampil **satu** sesuai state | `index.html:2243` |
| 10 | `fieldset[data-valve="1"]` | Fieldset Katup Valve 1 | Berisi mode switch + form | `index.html:2283` |
| 11 | `fieldset[data-valve="1"] .mode-switch-bar` | Mode kontrol | Dua chip: "Sensor Ambang (Otomatis)" / "Jadwal Waktu Tertentu" | `index.html:2286` |
| 12 | `fieldset[data-valve="1"] .mode-sensor` | Grup input mode sensor | `open_temp_threshold_c`, `open_soil_max_pct` | `index.html:2298` |
| 13 | `fieldset[data-valve="1"] .mode-schedule` | Grup input mode jadwal | Daftar window + tombol tambah | `index.html:2331` |
| 14 | `fieldset[data-valve="1"] .win-row` | Baris window jadwal | `input[type=time][data-field=start]`, `[data-field=end]`, tombol hapus `✕` | `index.html:2346` |
| 15 | `fieldset[data-valve="1"] [data-act="add-window"]` | "+ Tambah Window Baru" | Sisip baris default `06:00`–`06:15` | `index.html:2336`, `index.html:2377` |
| 16 | `.settings-card .form-error` | Banner error validasi | Teks berawalan `"Validasi: "` bila gagalnya client-side, atau pesan server apa adanya | `index.html:2394`, `index.html:2406` |
| 17 | `[data-act="save"]` | "Simpan Pengaturan" | Validasi client dulu, lalu `PUT /api/nodes/:id/config` | `index.html:2265`, `index.html:2387` |
| 18 | `[data-act="cancel"]` | "Batal Perubahan" | Re-render kartu dari `settingsState` (snapshot server) | `index.html:2266`, `index.html:2381` |
| 19 | `fieldset[data-valve="2"]` | Fieldset Katup Valve 2 | Serupa dengan valve 1 | `index.html:2261` |

Fieldset valve 1 **dan** valve 2 identik secara struktur; screenshot cukup menandai
fieldset 1 untuk_mgrcfg dan menyebut valve 2 sebagai pasangan.

Field `close_temp_threshold_c`, `close_soil_min_pct`, `max_open_duration_s` ada di
kedua mode (`index.html:2313-2329`); `open_temp_threshold_c` dan `open_soil_max_pct`
hanya relevan di mode sensor.

**Validasi client** (`clientValidate`, `index.html:2463-2490`) berjalan **sebelum** request,
dan pesannya selalu diawali `"Validasi: "`:

| Aturan | Pemicu |
|--------|--------|
| Maksimal 12 window per katup | `schedule.length > 12` |
| Window wajib diisi | `start`/`end` kosong |
| Mulai harus sebelum selesai | `start === end` |
| Melewati tengah malam | `start > end` |
| Window tumpang tindih | setelah sort by start |
| Duplikasi window | start & end identik |

Server memvalidasi ulang (`ValveConfig::validate`, `src/config.rs:100`) dengan aturan
identik, sehingga pesan bisa datang dari sisi client maupun server — dan bentuknya berbeda
(`"Validasi: ..."` vs pesan mentah). Uji harus membedakan keduanya.

`[data-act="cancel"]` memakai `card.outerHTML = settingsCardHtml(orig)`
(`index.html:2385`) — artinya kartu **diganti seluruhnya**, event delegation tetap jalan
karena delegasi event-nya terpasang di `#settings-nodes`, bukan per kartu.

---

## `07-modal-add-node` — Modal tambah node

| # | Selector | Label | Fungsi | Sumber |
|---|----------|-------|--------|--------|
| 1 | `#modal-add-node` | Overlay modal | `.modal-overlay`, `hidden` by default | `index.html:1430` |
| 2 | `.modal-dialog` | Dialog | Judul + deskripsi + form + aksi | `index.html:1431` |
| 3 | `.modal-title` | Judul "Tambah Node Baru" | | `index.html:1432` |
| 4 | `.modal-desc` | Deskripsi | "Daftarkan pengontrol irigasi baru…" | `index.html:1433` |
| 5 | `#new-node-id` | Input ID Node | Wajib diisi; validasi format hanya di **server** | `index.html:1436` |
| 6 | `.form-hint` di bawah `#new-node-id` | Hint format | "Gunakan huruf, angka, tanda hubung (-), atau garis bawah (_)." | `index.html:1437` |
| 7 | `#new-node-zone` | Input Zona | Opsional | `index.html:1441` |
| 8 | `#modal-add-node-err` | Banner error | `"ID Node wajib diisi!"` bila kosong (client), atau `body.error` dari server | `index.html:1443`, `index.html:2517` |
| 9 | `#btn-cancel-add-node` | Tombol "Batal" | Tutup modal **tanpa** request | `index.html:1445` |
| 10 | `#btn-submit-add-node` | Tombol "Simpan Node" | `POST /api/nodes`, tutup modal bila sukses | `index.html:1446` |

`openAddNodeModal()` mengosongkan kedua input dan menyembunyikan `#err` setiap kali dibuka
(`index.html:2498-2504`), jadi state modal tidak pernah membawa sisa input sebelumnya.

Modal ditutup dengan `hidden = true` (`index.html:2507`) — tidak ada animasi keluar,
dan tidak ada handler klik-overlay atau tombol Escape.

---

## `08-node-disabled` — Node nonaktif

State Pengaturan dengan satu node `enabled = false`.

| Selisih vs `06-settings` | Element | Sumber |
|-------------------------|---------|--------|
| Pill "**Dinonaktifkan**" (kelas `.offline`) | `.status-pill` | `index.html:2241` |
| Tombol "**Aktifkan**" menggantikan "Nonaktifkan" | `[data-act="activate"]` | `index.html:2245` |
| Node hilang dari grid `#nodes` | `#nodes` | — (`GET /api/nodes` default `include_disabled=false`) |
| Node **tetap** muncul di chip filter Data & Analitik | `#filter-node` | `index.html:2114` |

Baris terakhir mudah terlewat dan layak jadi test: halaman Data meminta
`/api/nodes?include_disabled=true` sementara halaman Kontrol tidak.

---

## Komponen Lintas Halaman

| Komponen | Perilaku |
|----------|----------|
| `.nav-tabs button.is-active` | Menandai tab aktif; `switchPage` juga men-toggle `hidden` section (`index.html:1572`) |
| `.status-pill` | Varian `online` / `offline` / `idle`. Di tabel data, `Keputusan` → `online`, `Pembacaan` → `idle` (`index.html:2163`) |
| `.chip-btn` / `.chip-btn.is-active` | Dipakai bersama oleh filter data **dan** mode switch valve; keduanya punya handler sendiri |
| `.live-dot` | Titik berdenyut pada brand, status online, dan item jadwal `running` |
| `#toasts .toast` | Auto-dismiss 3500 ms (`index.html:1519`) |
| `input:focus` | `outline: 2px solid var(--accent)` di login (`login.html:39`) |

## Detail yang Perlu Diperhatikan Saat Menulis Assertion

Tiga tempat di mana perilaku JS tidak didukung aturan CSS, atau tidak seperti yang
kita kira — penting supaya test tidak gagal karena asumsi yang keliru:

1. `.valve-card` menerima `is-open`, dan `.btn-release-auto` di-render kondisional di JS.
   Keduanya tidak punya aturan CSS khusus, jadi tampilan hanya berubah lewat `.micro-badge.manual-held`.
2. `#download-csv` handler di-assign ulang di setiap `renderDataTable()`
   (`index.html:2181`) via `.onclick`, bukan `addEventListener`.

Selain itu, tiga teks berikut memakai ejaan bahasa Indonesia yang tidak baku dan harus
dicocokkan **persis** bila jadi assertion:

| Teks di UI | Sumber |
|------------|--------|
| "Semua node normal terkoneksi" | `index.html:1656` |
| "Mengalir" | `index.html:1862` |
| "Mode sensor aktif (tidak ada jadwal waktu khusus)." | `index.html:1755` |