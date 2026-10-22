# Plan: Implementasi Sistem Digital Twin Irigasi (Rust, Edge-Only, Pi 3 + ESP32)

## Context

Direktori proyek ini masih kosong (hanya berisi `brief.md` — belum ada git repo, belum ada kode). Brief sudah mendefinisikan arsitektur high-level dan roadmap 4 fase (A–D), tapi belum diterjemahkan menjadi struktur proyek, skema data, dan urutan langkah konkret yang bisa langsung dieksekusi. Tujuan plan ini: mengubah brief menjadi rencana implementasi yang jelas, sesuai keputusan tim (scope penuh Fase A–D, GitLab untuk CI/CD, desain command dua-arah Pi↔ESP32 tetap dibangun dari awal meski valve fisik belum ada — hanya sensor+publish yang sudah jalan di ESP32 saat ini).

Kondisi environment dev machine (macOS, Apple Silicon) sudah dicek:
- `cargo`/`rustc` 1.95.0 sudah terpasang
- Docker daemon disediakan oleh **OrbStack** (bukan Docker Desktop), saat ini **belum running**
- `cross` belum terpasang, target `aarch64-unknown-linux-gnu` belum ditambahkan ke rustup
- Mosquitto & `mosquitto_pub`/`sub` sudah terpasang via Homebrew
- Hardware: Raspberry Pi 3 (OS Lite 64-bit) dan minimal 1 ESP32 sudah siap fisik; ESP32 baru bisa baca sensor + publish MQTT (belum ada valve/solenoid terpasang)
- Topic MQTT sudah ditentukan tim: prefix `irigasi/ipesg/v1/...` (`v1` = versi skema/protokol, bukan ID node). Tiap ESP32 punya **2 sensor suhu + 2 sensor kelembapan tanah** (tanpa sensor kelembapan udara).

**Keputusan yang sudah dikonfirmasi** (dari firmware `iot.c` yang sudah jalan di hardware): **1 ESP32 mengontrol 2 valve independen** — `RELAY3` digerakkan oleh pasangan sensor (`soil3`/`suhu3`), `RELAY4` oleh pasangan sensor (`soil4`/`suhu4`). Firmware sudah diubah supaya kedua relay ini murni dieksekusi dari command MQTT Pi (`valve_id: 1|2`), bukan lagi diputuskan lokal di ESP32 — lihat §2 dan §4.

**Broker MQTT: Mosquitto lokal di Pi** (sesuai rencana awal brief.md, "edge-only, tanpa cloud"). Sempat dicoba pakai `broker.emqx.io` (broker publik) untuk kemudahan testing awal (ESP32 & Pi tidak perlu 1 LAN), tapi **dibalik lagi** setelah terbukti tidak reliable: client `rumqttc` di Pi berulang kali gagal dapat `SUBACK` dari broker publik ini setiap kali service di-restart (walau `mosquitto_sub` biasa selalu berhasil instan di broker yang sama, jadi bukan masalah jaringan) — begitu pindah ke Mosquitto lokal, subscribe langsung normal. Konsekuensi: ESP32 & Pi wajib 1 jaringan WiFi yang sama (`mqtt_server` di `iot.c` = IP LAN Pi), `config.pi.toml` pakai `broker_host = "localhost"`.

---

## 1. Struktur Proyek Rust

Satu Cargo package (bukan workspace — tidak perlu multi-crate untuk MVP single-binary), nama package/binary: **`irigasi-core`**.

```
irigasi/
├── Cargo.toml
├── .gitignore
├── .gitlab-ci.yml
├── config/default.toml        # broker addr, sqlite path, zone registry, thresholds
├── src/
│   ├── main.rs                 # load config, init tracing, jalankan pipeline
│   ├── lib.rs
│   ├── config.rs               # struct Config (serde) + load TOML + env override
│   ├── node.rs                 # DeviceId + registry dari config (1 device = 1 zona untuk MVP)
│   ├── mqtt/{mod,client,topics}.rs   # koneksi rumqttc, subscribe irigasi/ipesg/v1/+/sensor
│   ├── model/{sensor,command}.rs     # struct SensorReading (2 suhu + 2 kelembapan) & ValveCommand (serde)
│   ├── twin/{mod,water_balance}.rs   # state per-device in-memory, accumulator (dicatat, belum menggerakkan keputusan di MVP)
│   ├── decision/{mod,rules}.rs       # evaluate() pure function: threshold + cooldown -> Action
│   ├── storage/{mod,db,repo,schema.sql}  # rusqlite, init idempotent
│   └── dashboard/{mod,routes}.rs     # di balik Cargo feature "dashboard" (off by default)
├── static/index.html            # hanya dipakai jika feature dashboard aktif
└── tests/decision_rules.rs      # unit test rules.rs
```

Dependency inti: `tokio`, `rumqttc` (client MQTT — broker tetap Mosquitto sesuai brief), `serde`/`serde_json`, `rusqlite` (feature `bundled`, supaya tidak perlu apt-install libsqlite3 di Pi), `toml`, `anyhow`/`thiserror`, `tracing`/`tracing-subscriber` (stdout, otomatis ditangkap `journald` lewat systemd), `chrono`, `uuid`. `axum` hanya di bawah `--features dashboard`.

## 2. Skema Topic & Payload MQTT

Prefix sesuai yang sudah dipakai tim: **`irigasi/ipesg/v1/...`** (`v1` = versi skema, tetap dipakai sampai skema payload berubah besar — bukan pengganti device_id). Identitas node ditambahkan sebagai segmen topic di bawah versi, karena `v1`/`v2` sendiri bukan ID node:

| Topic | Arah | QoS | Retain |
|---|---|---|---|
| `irigasi/ipesg/v1/<device_id>/sensor` | ESP32→Pi | 1 | false |
| `irigasi/ipesg/v1/<device_id>/command` | Pi→ESP32 | 1 | **false** (retained command bisa bikin valve "stuck open" saat ESP32 reboot) |
| `irigasi/ipesg/v1/<device_id>/status` | ESP32→Pi (LWT) | 1 | true |
| `irigasi/ipesg/v1/<device_id>/command/ack` | ESP32→Pi | 1 | false (reserved, dipakai setelah firmware handle command) |

`device_id` belum ada konvensi final dari tim — diusulkan string pendek seperti `esp32-01`, `esp32-02` (gampang diganti, cuma field config, bukan keputusan yang mengunci arsitektur). Pi subscribe wildcard `irigasi/ipesg/v1/+/sensor` supaya menambah node baru = edit config, bukan kode.

Firmware ESP32 sebaiknya punya watchdog lokal (auto-close setelah `duration_s` walau koneksi putus) — ini menjawab poin terbuka #4 di brief (fallback saat Pi down), meski implementasinya di luar scope Rust plan ini.

Payload sensor (ESP32→Pi) — **wire format aktual dari firmware `iot.c`** (nama field asli dipertahankan di ESP32, bukan direname, supaya tidak perlu reflash tiap kali skema Pi berubah):
```json
{"device_id":"esp32-02","soil3_pct":20,"status3":"Kering","soil4_pct":60,"status4":"Basah",
 "suhu3":29.5,"suhu4":22.0,"relay3":"OFF","relay4":"OFF"}
```
Pi (`model/sensor.rs`, `SensorReading`) memetakan via `#[serde(rename = "...")]`: `soil3_pct → soil_moisture_1_pct`, `soil4_pct → soil_moisture_2_pct`, `suhu3 → temp_1_c`, `suhu4 → temp_2_c` (valve 1 ↔ pasangan sensor 3, valve 2 ↔ pasangan sensor 4, konsisten dengan §4). Field `status3`/`status4`/`relay3`/`relay4` diabaikan Pi (serde default: field ekstra di JSON yang tidak ada di struct diabaikan, tidak error). `seq`/`uptime_ms` **belum dikirim firmware saat ini** — dibuat opsional (`Option<i64>`, default `None`) di struct supaya tetap kompatibel; `received_at` (stempel waktu Pi sendiri) tetap jadi timestamp kanonik di `readings`.

Payload command (Pi→ESP32) — `valve_id` **wajib diisi** (`1` untuk RELAY3/pasangan sensor 1, `2` untuk RELAY4/pasangan sensor 2), karena firmware sudah diimplementasikan memetakan langsung `valve_id` ke relay tersebut (lihat `applyValveCommand()` di `iot.c`):
```json
{"device_id":"esp32-01","command_id":"c-3f9a1b2c","action":"open","duration_s":120,
 "valve_id":1,"issued_at":1735459300,"reason":"soil_moisture_below_threshold"}
```
`duration_s` di firmware berfungsi sebagai **safety-net auto-off** (relay otomatis mati kalau Pi lupa kirim `close`), bukan mekanisme utama penutupan — Pi tetap mengirim command `action:"close"` eksplisit begitu kondisi tutup terpenuhi (§4).

## 3. Skema SQLite

Di-embed sebagai `src/storage/schema.sql`, dijalankan idempotent (`CREATE TABLE IF NOT EXISTS`) tiap startup — tanpa migration framework dulu.

- `readings` — semua data sensor mentah (device_id, seq, uptime_ms, `soil_moisture_1_pct`, `soil_moisture_2_pct`, `temp_1_c`, `temp_2_c`, received_at + index by device+time)
- `twin_state` — 1 baris per device: reading terakhir (kedua sensor), `valve_1_open`/`valve_2_open` (boolean, status valve terakhir yang berhasil dipublish — dipakai mode `schedule` supaya tidak kirim `open` berulang selama masih dalam 1 window jadwal yang sama), `water_balance_mm` (placeholder, belum dipakai decision engine)
- `decisions` — **setiap** evaluasi rule engine dicatat per valve (2 baris per reading, satu untuk `valve_id=1` satu untuk `valve_id=2`), termasuk outcome `none` (action, duration_s, valve_id, reason, `trigger_soil_moisture_pct`, `trigger_temp_c`, `threshold_used`, `published` flag) — jadi log keputusan valve sekaligus dataset berlabel untuk upgrade ke ML nanti.
- `command_acks` — reserved, belum dipakai sampai firmware ESP32 kirim ack

## 4. Rule-Based Decision Engine

**2 valve independen per node, dievaluasi terpisah** — valve 1 dari pasangan (`soil_moisture_1_pct`, `temp_1_c`), valve 2 dari pasangan (`soil_moisture_2_pct`, `temp_2_c`). Ini memindahkan logika yang sebelumnya hardcoded di firmware ESP32 ke Pi, supaya threshold (dan sekarang juga jadwal) bisa di-tuning tanpa reflash. **Tiap valve punya `mode` sendiri** — `"sensor"` (threshold suhu+kelembapan) atau `"schedule"` (buka per jam) — dipilih di config, tidak dicampur logikanya dalam satu valve:

```rust
// decision/rules.rs
pub fn evaluate(input: &DecisionInput, node_cfg: &NodeConfig, twin: &TwinState, now: DateTime<Utc>) -> Vec<DecisionOutcome> {
    vec![
        evaluate_valve(1, input.soil_moisture_1_pct, input.temp_1_c, &node_cfg.valve_1, twin.valve_1_open, now),
        evaluate_valve(2, input.soil_moisture_2_pct, input.temp_2_c, &node_cfg.valve_2, twin.valve_2_open, now),
    ]
}

fn evaluate_valve(valve_id: u8, soil_pct: f32, temp_c: f32, cfg: &ValveConfig, is_open: bool, now: DateTime<Utc>) -> DecisionOutcome {
    // Kondisi CLOSE selalu dicek lebih dulu & berlaku di kedua mode (sesuai keputusan tim)
    if temp_c < cfg.close_temp_threshold_c && soil_pct >= cfg.close_soil_min_pct {
        return DecisionOutcome::close(valve_id, "close_condition_met");
    }
    match cfg.mode {
        Mode::Sensor => {
            if temp_c >= cfg.open_temp_threshold_c && soil_pct <= cfg.open_soil_max_pct {
                return DecisionOutcome::open(valve_id, cfg.max_open_duration_s, "soil_moisture_below_threshold");
            }
        }
        Mode::Schedule => {
            if !is_open && is_within_schedule(now, &cfg.schedule) {
                return DecisionOutcome::open(valve_id, cfg.max_open_duration_s, "scheduled_window");
            }
        }
    }
    DecisionOutcome::none(valve_id)
}
```

**Mode `"sensor"`** — mereplikasi persis kondisi hysteresis yang sudah teruji di firmware asli (`iot.c` sebelum diubah), supaya perilaku sistem tidak berubah saat pindah dari ESP32 ke Pi:
- **Open** kalau `temp_c >= open_temp_threshold_c` **DAN** `soil_moisture_pct <= open_soil_max_pct` (default: `27.0`, `35`)
- **Close** kalau `temp_c < close_temp_threshold_c` **DAN** `soil_moisture_pct >= close_soil_min_pct` (default: `25.0`, `45`)
- Gap antara `35–45` (soil) dan `25–27` (suhu) jadi mekanisme anti-flapping (hysteresis band) — menggantikan rencana awal `min_reopen_interval_s` (cooldown berbasis waktu tidak diperlukan lagi).

**Mode `"schedule"`** (baru, sesuai keputusan tim) — buka valve otomatis di jam-jam yang dikonfigurasi, tapi **penutupan tetap pakai kondisi close sensor yang sama** (bukan durasi jadwal): `is_within_schedule()` cek apakah `now` (jam lokal Pi) masuk salah satu window `[start, end)` di config, dan valve belum `is_open` (supaya command `open` tidak dikirim berulang tiap reading selama masih dalam 1 window yang sama). `twin_state.valve_{1,2}_open` (§3) dipakai untuk melacak status ini — diupdate tiap kali command `open`/`close` berhasil dipublish.

Threshold & jadwal dikonfigurasi via `config/default.toml`, per-node per-valve:
```toml
[[nodes]]
id = "esp32-01"

[nodes.valve_1]              # contoh: mode sensor (default)
mode = "sensor"
open_temp_threshold_c = 27.0
open_soil_max_pct = 35.0
close_temp_threshold_c = 25.0
close_soil_min_pct = 45.0
max_open_duration_s = 120

[nodes.valve_2]              # contoh: mode jadwal
mode = "schedule"
close_temp_threshold_c = 25.0
close_soil_min_pct = 45.0
max_open_duration_s = 900
schedule = [
  { start = "06:00", end = "06:15" },
  { start = "17:00", end = "17:15" },
]
```
`max_open_duration_s` dikirim sebagai `duration_s` di command — di kedua mode ini cuma **batas atas aman** (firmware auto-off kalau `close` tidak pernah datang), bukan mekanisme penutupan utama; Pi tetap mengirim `action:"close"` eksplisit begitu kondisi close (sensor) terpenuhi pada evaluasi berikutnya.

Flag `dry_run` (global di `[decision]`, kalau true: hitung & log saja, tidak publish MQTT command) tetap ada — cara aman uji ubah threshold/jadwal sebelum yakin dengan perilakunya di Pi.

Engine event-driven (dipicu tiap sensor reading masuk, bukan polling — termasuk untuk mode `schedule`, jadi jadwal baru dievaluasi saat reading berikutnya masuk, bukan timer terpisah; cukup akurat karena interval publish ESP32 ~2 detik). Inti logic per valve adalah pure function `evaluate_valve(...)`, gampang di-unit-test dan jadi titik ganti kalau nanti upgrade ke model statistik/ML (Fase D). **Setiap** outcome (termasuk `None`) ditulis ke tabel `decisions`, untuk kedua valve setiap kali ada reading masuk; kalau publish MQTT gagal (broker unreachable), service tidak boleh crash — tetap log dengan `published=0`.

`twin::water_balance` sengaja dibangun & dicatat tapi **belum** dipakai untuk menggerakkan keputusan di MVP — ini seam terbuka untuk Fase D (mis. mengganti kondisi hysteresis di atas dengan model water-balance yang lebih akurat).

## 5. Fase A — Buktikan Konsep (langkah konkret)

1. **Toolchain**: start OrbStack (`open -a OrbStack`) → `docker info` untuk verifikasi; `rustup target add aarch64-unknown-linux-gnu`.
2. `cargo init --name irigasi-core`, scaffold modul sesuai §1, tambah dependency §1.
3. `cargo build`/`cargo run` native di macOS dulu — pastikan compile sebelum cross-compile.
4. Uji MQTT round-trip tanpa ESP32: `brew services start mosquitto` (broker lokal di laptop, cuma untuk uji unit), publish dummy pakai `mosquitto_pub` ke `irigasi/ipesg/v1/esp32-01/sensor`, verifikasi command muncul di `mosquitto_sub` topic `.../command`.
5. Cek isi `readings`/`decisions` di SQLite lokal setelah step 4.
6. **Cross-compile**: tool `cross` (Docker-based) **tidak dipakai** — image resminya (`ghcr.io/cross-rs/aarch64-unknown-linux-gnu`) ternyata cuma ada varian amd64, jadi kalau dipaksa jalan di Mac Apple Silicon akan lewat emulasi QEMU (lambat, dan sempat bikin disk penuh saat pull image besar). Solusi yang dipakai: `docker run` pakai image resmi **`rust:1.95`** (multi-arch, ada native arm64) + install `gcc-aarch64-linux-gnu`/`libc6-dev-arm64-cross` via `apt` di dalam container — persis pendekatan CI di §6, dijalankan lokal:
   ```bash
   docker run --rm -v "$PWD":/work -v irigasi-cargo-registry:/usr/local/cargo/registry -w /work rust:1.95 bash -c '
     apt-get update -qq && apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross pkg-config
     rustup target add aarch64-unknown-linux-gnu
     export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
     cargo build --release --target aarch64-unknown-linux-gnu'
   ```
   Verifikasi `file target/aarch64-unknown-linux-gnu/release/irigasi-core` = ELF aarch64. Volume `irigasi-cargo-registry` menyimpan cache crates.io supaya build berikutnya jauh lebih cepat.
7. **Setup Pi** (`ssh <user>@<IP Pi>`, cek `uname -m` = `aarch64`): install Mosquitto (`apt install mosquitto mosquitto-clients`), listener LAN (`listener 1883 0.0.0.0`, `allow_anonymous true` — cukup untuk MVP WiFi-lokal), `systemctl enable --now mosquitto`, `mkdir -p ~/app/data`.
8. `scp` binary + `deploy/config.pi.toml` (broker_host="localhost") ke Pi sebagai `~/app/config.toml`, jalankan manual dulu (foreground) pastikan connect MQTT sukses tanpa crash.
9. Deploy systemd unit `deploy/irigasi.service` (`User=irigasi`, `WorkingDirectory=~/app`, `Restart=on-failure`, `After=network-online.target Requires=mosquitto.service`), `enable --now`, pantau `journalctl -u irigasi -f`.
10. **Uji end-to-end dengan ESP32 asli**: firmware (`iot.c`, `mqtt_server` = IP LAN Pi) & Pi connect ke Mosquitto lokal yang sama, cocokkan nama topic & field JSON dengan §2, pantau reading masuk + decision ter-log di journalctl & SQLite. Actuation valve fisik **sengaja ditunda** — Fase A berhenti di "Pi menghitung & mem-publish/log command", bukan "valve bergerak", karena hardware valve belum ada.

**Catatan dari implementasi nyata**: broker.emqx.io (public) sempat dicoba dulu untuk Fase A (lihat Context) tapi dibalik ke Mosquitto lokal karena `rumqttc` gagal dapat `SUBACK` dari broker publik itu setiap restart. Device_id fisik yang benar-benar dipakai ternyata `esp32-01` (bukan `esp32-02` seperti asumsi awal) — `config.pi.toml` mendaftarkan **kedua-duanya** supaya fleksibel kalau ada unit ke-2.

**Akses SSH ke Pi**: dipakai key khusus (`deploy/.ssh/irigasi_pi`, di-gitignore) yang di-install ke Pi via `ssh-copy-id` sekali di awal (autentikasi awal pakai password), bukan reuse key pribadi laptop — mengikuti prinsip yang sama seperti brief step 14 (SSH key khusus, bukan personal).

## 6. Fase B — CI/CD Build Otomatis (GitLab)

1. `git init`, `.gitignore` (`/target`, `data/*.db`, `*.log`), buat project di GitLab (gitlab.com), push `main`.
2. **Catatan penting**: contoh `.gitlab-ci.yml` di brief pakai `cross` di dalam `image: rust:latest` — ini gagal di shared runner GitLab.com biasa karena `cross` butuh akses Docker daemon (perlu Docker-in-Docker/privileged runner). Untuk hindari kompleksitas itu di MVP, pipeline pakai cross-linker langsung (bukan `cross`) — `cross` tetap dipakai untuk build lokal di laptop (OrbStack sudah sedia Docker):
   ```yaml
   build:
     image: rust:1.95
     before_script:
       - apt-get update && apt-get install -y gcc-aarch64-linux-gnu libc6-dev-arm64-cross pkg-config
       - rustup target add aarch64-unknown-linux-gnu
     variables:
       CC_aarch64_unknown_linux_gnu: aarch64-linux-gnu-gcc
       CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: aarch64-linux-gnu-gcc
     script:
       - cargo build --release --target aarch64-unknown-linux-gnu
     artifacts:
       paths: [target/aarch64-unknown-linux-gnu/release/irigasi-core]
   ```
   (`rusqlite` feature `bundled` tetap compile lancar lewat `cc`-rs + linker di atas.) Pin image `rust:1.95` biar sama dengan rustc lokal.
3. Verifikasi artifact CI = build manual (brief step 12): tambah `--version` (embed git SHA via `build.rs`), bandingkan output binary hasil CI vs hasil `cross build` lokal, smoke-test keduanya lawan Mosquitto lokal (ulang step Fase A #4).

## 7. Fase C — Deploy Otomatis — sudah diimplementasi (2026-09-07)

Ikuti rekomendasi brief sendiri: **manual pull dulu**, bukan self-hosted runner.

1. `deploy/deploy.sh` (jalan di Pi): `curl` artifact terbaru dari GitLab job-artifact URL (`GET /projects/:id/jobs/artifacts/:ref/download?job=build`, header `PRIVATE-TOKEN`) pakai project Access Token scope `read_api` disimpan di `~/.irigasi-deploy-token` (chmod 600 — bukan token pribadi, sesuai prinsip brief step 14), extract binary dari zip, backup binary lama (`irigasi-core.bak-<timestamp>`, disimpan cuma 3 terakhir), `install -m 755` ke `/home/irigasi/app/irigasi-core`, `systemctl restart irigasi.service`, print `--version` (git SHA via `build.rs`) untuk verifikasi.
2. Jalankan manual via SSH setiap habis pipeline sukses. Baru pertimbangkan cron/otomatisasi lebih lanjut kalau proses manual mulai terasa merepotkan (sesuai brief).
3. **Alternatif terdokumentasi untuk nanti**: self-hosted GitLab Runner di LAN Pi (tag `local-pi-network`) + SSH key khusus CI (`gitlab-ci-deploy`, idealnya dibatasi `command=` di `authorized_keys` agar hanya bisa jalankan `deploy.sh`) — ini jalur upgrade kalau frekuensi push naik.
4. **Perbaikan sekaligus**: `.gitlab-ci.yml` sebelumnya build **tanpa** `--features dashboard` — artifact CI tidak pernah sama dengan binary yang sebenarnya jalan di Pi (yang selalu di-cross-compile manual dengan flag itu). Sekarang disamakan.

### 7.1 Runbook: Pi baru diinstall dari nol

Urutan lengkap dari Raspberry Pi OS Lite (64-bit) baru sampai service jalan:

1. **Flash & boot Pi** — Raspberry Pi Imager, aktifkan SSH + WiFi di step imager (atau `raspi-config` manual), user `irigasi` sesuai `deploy/irigasi.service` (`User=irigasi`).
2. **(Opsional tapi disarankan) pasang key deploy** supaya langkah berikutnya tidak perlu password tiap kali: `ssh-copy-id -i deploy/.ssh/irigasi_pi.pub irigasi@<ip-pi>` dari laptop dev (private key `deploy/.ssh/irigasi_pi` sengaja `.gitignore`d, jangan pernah commit).
3. **Salin folder deploy/ ke Pi**: `scp -r deploy irigasi@<ip-pi>:~/deploy-staging` (isinya `pi-setup.sh`, `deploy.sh`, `irigasi.service`, `config.pi.toml`).
4. **Jalankan setup dasar** (SSH ke Pi): `cd ~/deploy-staging && ./pi-setup.sh` — install `mosquitto`+`mosquitto-clients`+`unzip`+`curl`, siapkan `/home/irigasi/app/{,data}`, pasang `config.toml` (dari `config.pi.toml`, tidak menimpa kalau sudah ada), pasang & enable `irigasi.service` (belum di-start — binary belum ada).
5. **Buat token deploy** (sekali per Pi, manual lewat web GitLab — tidak bisa diotomasi dari CLI): GitLab project → Settings → Access Tokens → scope `read_api`, role Reporter. Simpan di Pi: `echo '<token>' > ~/.irigasi-deploy-token && chmod 600 ~/.irigasi-deploy-token`.
6. **Ambil & jalankan binary terbaru**: `~/deploy-staging/deploy.sh` — pull artifact job `build` dari branch `main`, pasang, restart service, print git SHA untuk verifikasi.
7. **Verifikasi**: `systemctl status irigasi.service`, `journalctl -u irigasi.service -f` (lihat log "Terhubung ke MQTT broker..." & "Dashboard HTTP aktif..."), buka `http://<ip-pi>:8080/` dari browser di LAN yang sama.
8. **Update berikutnya** cukup ulangi step 6 (`./deploy.sh`) setiap habis pipeline GitLab sukses — tidak perlu ulang step 1-5.

## 8. Fase D — Landasan untuk Scale-Up (bangun sekarang, manfaatkan nanti)

Desain di Fase A–C sudah menyiapkan seam berikut tanpa perlu dibangun sekarang:
- `[[nodes]]` array + wildcard subscribe `irigasi/ipesg/v1/+/sensor` → tambah node = edit config, bukan kode
- `evaluate_valve()` pure function terpisah dari MQTT/storage → ganti rule-based ke model statistik/ML (`linfa`, sesuai catatan brief) cukup sentuh `decision/rules.rs`
- Kolom audit di tabel `decisions` (`trigger_soil_moisture_pct`, `trigger_temp_c`, `threshold_used`) sudah jadi dataset berlabel untuk ML nanti
- `twin::water_balance` yang sudah dicatat (belum dipakai) siap jadi basis keputusan yang lebih canggih
- `device_id`/`seq`/`uptime_ms` yang sudah direkam dari awal → visibilitas kesehatan node saat 50-node scale
- Boundary `storage/mod.rs` siap disisipi channel + batched write kalau volume 50 node jadi bottleneck
- `--features dashboard` menjaga binary MVP tetap ramping sampai benar-benar perlu dashboard lengkap
- Update dokumen proposal (roadmap item 19) — item non-teknis, tugas terpisah untuk sisi penulisan, bukan dependency arsitektur.

### 8.1 Dashboard + API — sudah diimplementasi

`--features dashboard` sekarang benar-benar berisi kode (sebelumnya cuma dependency kosong). Desain kunci:
- **`Twin` in-memory di-share** (`Arc<Mutex<Twin>>`) antara loop MQTT dan dashboard — direvisi dari desain awal "tidak share" karena itu justru akar bug "status valve manual ketimpa auto dalam ~2 detik" (lihat catatan fix di bawah). Dashboard tetap baca snapshot lewat **koneksi SQLite kedua** (`storage::db::open` dipanggil lagi) untuk `GET /api/nodes` dkk., tapi *tulis* (`post_command`/`release_valve`) sekarang update `Twin` bersama juga supaya decision engine langsung tahu ada override manual. `storage/db.rs` set `PRAGMA journal_mode=WAL` + `busy_timeout=5000` supaya 2 connection ini tidak `SQLITE_BUSY`.
- **HTML di-embed ke binary** (`include_str!`) — `static/index.html` tidak perlu di-scp ke Pi, deploy tetap 1 binary + 1 config.toml. `tower-http` dihapus dari `Cargo.toml` (tidak kepakai).
- Endpoint: `GET /api/nodes` (snapshot semua node), `GET /api/nodes/:id/config` (threshold, read-only — sengaja tidak expose `MqttConfig`/credential), `GET /api/nodes/:id/readings|decisions?limit=` (riwayat), `POST /api/nodes/:id/command` (override manual — publish MQTT + catat ke `decisions` dengan `reason="manual_override"` + update `twin_state` DB & `Twin` in-memory).
- **Tanpa autentikasi** (keputusan sadar — LAN-only, sama seperti asumsi trust MQTT `allow_anonymous`).
- Config baru `[dashboard]` (`enabled`, `bind_addr`) di `config/default.toml` & `deploy/config.pi.toml`.
- Cross-compile Pi sekarang pakai `cargo build --release --features dashboard --target aarch64-unknown-linux-gnu`.
- Live di Pi: `http://192.168.137.251:8080/` (IP DHCP, cek ulang di router/`hostname -I` di Pi kalau berubah lagi).

#### 8.1.1 Fix: status valve tidak sinkron & manual override ketimpa (2026-09-04)

Gejala: tombol valve di dashboard kembali ke OFF dalam hitungan detik meski relay fisik ON; klik manual di satu valve terlihat mempengaruhi valve lain.

Root cause (3 hal, semua sisi Rust — firmware `iot.c` sudah benar):
1. `SensorReading` mengabaikan field `relay3`/`relay4` yang sudah dikirim firmware di payload sensor yang sama (lihat `iot.c` §Publish) — status ON/OFF dashboard jadi murni asumsi in-memory, bukan ground truth relay.
2. `AppState` dashboard tidak share `Twin` in-memory dengan loop MQTT (lihat desain awal §8.1 di atas) — `post_command`/`release_valve` cuma menulis SQLite, sehingga `upsert_twin_state()` di reading berikutnya (tiap ~2 detik) menimpa balik status manual dengan state default/auto dari `Twin` yang tidak tahu apa-apa soal override.
3. `decision::rules::evaluate_valve` tidak pernah membaca `valve_x_source` — decision engine tetap bebas menerbitkan command auto walau valve sedang di-override manual.

Fix (`src/model/sensor.rs`, `src/twin/mod.rs`, `src/decision/rules.rs`, `src/lib.rs`, `src/dashboard/{state,routes}.rs`):
- `SensorReading` parse `relay3`/`relay4` → `Twin::update_from_reading()` jadikan itu sumber kebenaran `valve_x_open` di tiap reading.
- `Twin` sekarang `Arc<Mutex<Twin>>` di-share ke `AppState` — `post_command` & `release_valve` update `Twin` in-memory selain SQLite.
- `evaluate_valve_guarded()` menahan decision engine (`Action::None`, reason `"manual_hold"`) selama `source == "manual"`, sampai dilepas via `release_to_auto()`.
- Diverifikasi live di Pi (192.168.137.251) pasca-deploy: `valve_2_source` esp32-02 bertahan `"manual"` lintas >90 detik & banyak reading (sebelum fix, flip balik ke `"auto"` dalam ~2 detik).

**Redesign tampilan** (referensi visual dari mockup Claude Design Canvas, dibangun ulang sebagai vanilla JS tanpa CDN/runtime proprietary): 2 tab ("Kontrol & Pantau" + "Data & Unduh"), kartu KPI, kartu per-node (bar kelembapan+threshold, valve toggle dengan durasi 60s/2mnt/5mnt), badge Otomatis/Manual per valve, tombol "Lepas ke Otomatis", tabel riwayat dengan filter node/jenis/rentang + export CSV client-side. Penambahan untuk mendukung ini:
- `NodeConfig.zone: Option<String>` (label plot, mis. "Zona A") di `config.rs` + config files.
- `twin_state` dapat 4 kolom baru: `valve_{1,2}_source` ('auto'/'manual'), `valve_{1,2}_expires_at` (epoch, dipakai hitung sisa waktu di frontend). Migrasi kolom untuk DB lama ditangani `storage::db::migrate_twin_state_columns` (cek `pragma_table_info` dulu — SQLite tidak punya `ADD COLUMN IF NOT EXISTS`).
- Endpoint baru `POST /api/nodes/:id/valves/:valve_id/release` — lepas valve ke mode auto tanpa publish MQTT (DB-only).
- `GET /api/nodes` sekarang selalu mengembalikan seluruh node dari config (bukan cuma yang sudah punya baris `twin_state`), plus field `online` (heuristik: `last_reading_at` dalam 30 detik terakhir).
- Sengaja di-skip dari mockup: sparkline riwayat per-kartu (butuh N+1 fetch tambahan) dan toggle mode Otomatis/Manual global (tidak ada konsepnya di backend — tiap valve independen).

### 8.2 Rencana: aktifkan mode "Jadwal Siram" (belum diterapkan)

Permintaan: valve buka sesuai jam siram tetap, tapi **berhenti** tetap berdasarkan threshold sensor (bukan durasi jadwal habis). Setelah dicek: **fitur ini sudah ada dan lengkap** sejak Fase A — `ValveConfig::Schedule` (`src/config.rs`) + `evaluate_valve()` (`src/decision/rules.rs`) sudah persis mengimplementasikan perilaku ini, dilindungi 4 unit test (`schedule_mode_opens_within_window_when_not_already_open`, `schedule_mode_does_not_reopen_if_already_open`, `schedule_mode_close_condition_overrides_schedule`, `schedule_mode_none_outside_window`). Hanya saja **belum pernah diaktifkan** — `config/default.toml` & `deploy/config.pi.toml` saat ini keempat blok valve (`esp32-01`/`esp32-02` × `valve_1`/`valve_2`) masih `mode = "sensor"`.

Jadi ini **bukan pekerjaan kode**, murni **perubahan konfigurasi** (tidak perlu build ulang binary). Keputusan yang sudah dikonfirmasi user:
- Kondisi berhenti **tetap** logika yang sudah ada: `temp_c < close_temp_threshold_c DAN soil_pct >= close_soil_min_pct` (AND, bukan cuma soil) — tidak ada perubahan di `decision/rules.rs`.
- Jam siram pakai **placeholder** dulu, gampang diedit sendiri nanti: `06:00–06:15` dan `17:00–17:15`.

**Perubahan di kedua config file**, untuk semua 4 blok valve, dari:
```toml
[nodes.valve_1]
mode = "sensor"
open_temp_threshold_c = 27.0
open_soil_max_pct = 35.0
close_temp_threshold_c = 25.0
close_soil_min_pct = 45.0
max_open_duration_s = 120
```
menjadi:
```toml
[nodes.valve_1]
mode = "schedule"
close_temp_threshold_c = 25.0
close_soil_min_pct = 45.0
max_open_duration_s = 120

[[nodes.valve_1.schedule]]
start = "06:00"
end = "06:15"

[[nodes.valve_1.schedule]]
start = "17:00"
end = "17:15"
```
(`open_temp_threshold_c`/`open_soil_max_pct` dihapus — field itu tidak ada di varian `Schedule`, `ValveConfig` internally-tagged by `mode` jadi field asing akan ditolak serde. `close_temp_threshold_c`/`close_soil_min_pct`/`max_open_duration_s` tetap nilai sama seperti sekarang, cuma mekanisme buka yang berubah dari threshold jadi jadwal.)

**Deploy**: config-only, tidak perlu cross-compile — `scp deploy/config.pi.toml` ke `/home/irigasi/app/config.toml`, `systemctl restart irigasi`, cek log bersih.

**Verifikasi sebelum deploy ke Pi**:
1. `cargo run` lokal dengan config terupdate — pastikan `Config::load()` sukses parse (kalau field `open_*` kelupaan tidak dihapus, serde akan gagal parse dengan jelas).
2. Uji "buka sesuai jadwal": ganti sementara 1 window schedule di config lokal supaya mencakup jam saat ini, jalankan service, `mosquitto_pub` dummy reading yang tidak memicu close, cek log `action=Open reason=scheduled_window`.
3. Uji "tutup tetap by sensor": dari state open di atas, `mosquitto_pub` reading dengan soil ≥ close_soil_min_pct DAN temp < close_temp_threshold_c → cek log `action=Close reason=close_condition_met` (membuktikan penutupan tidak menunggu jadwal habis).
4. `cargo test` tetap harus lolos (regression check, tidak ada test baru yang perlu ditambah).
5. Kembalikan ke jam placeholder asli (bukan jam uji), baru redeploy ke Pi, lalu pantau `journalctl -u irigasi -f` melewati jam siram sungguhan pertama untuk konfirmasi end-to-end di hardware asli.

---

## Verifikasi

- **Fase A**: `mosquitto_pub` dummy → command muncul di `mosquitto_sub` + baris baru di `readings`/`decisions` (SQLite) — lokal dulu, lalu ulangi persis dengan Pi + ESP32 fisik dan cek `journalctl -u irigasi -f`.
- **Fase B**: bandingkan `--version` & smoke-test MQTT antara binary hasil `cross build` lokal vs artifact CI GitLab — harus identik perilakunya.
- **Fase C**: jalankan `deploy.sh` manual di Pi setelah pipeline sukses, konfirmasi `systemctl status irigasi.service` restart dengan binary versi baru (cek `--version`).
- **Unit test**: `cargo test` untuk `decision/rules.rs` (kasus: di bawah threshold → Open, di atas → None, dalam cooldown window → None meski di bawah threshold).
