# Brief Teknis & Plan: Sistem Digital Twin Irigasi Presisi (Edge-Only, Raspberry Pi 3 Lite 64-bit + Rust)

**Tanggal:** 29 Agustus 2026
**Status:** Draft untuk diskusi & perencanaan tim
**Scope:** MVP (Minimum Viable Product) — bukan versi final/produksi

> Dokumen ini mencakup keseluruhan gambaran: dari konsep sistem, arsitektur perangkat, stack teknis, roadmap kerja bertahap, sampai rencana CI/CD. Fokus MVP: buktikan 1 jalur data end-to-end (1 ESP32 ↔ 1 Pi 3) dulu sebelum scale-up.

---

## 1. Latar Belakang & Perubahan Arsitektur

Proposal awal (Gambar 7) merancang sistem berbasis cloud (AWS/Azure) sebagai pusat pemrosesan digital twin dan AI/ML. Setelah diskusi, arsitektur direvisi menjadi **full edge computing di Raspberry Pi 3**, tanpa ketergantungan cloud, dengan alasan:

- Lahan pertanian sering minim/tidak stabil koneksi internet
- Mengurangi biaya operasional (tidak ada biaya cloud)
- Latensi kontrol lebih rendah (keputusan diambil lokal)
- Cocok untuk skala prototipe/riset (TKT 6–7)

Untuk mengejar performa pada perangkat terbatas (Pi 3: 1GB RAM, quad-core Cortex-A53 ~1.2GHz), bahasa pemrograman inti diputuskan menggunakan **Rust (compiled binary)**, bukan Python, guna meminimalkan overhead memori dan CPU.

---

## 2. Pembagian Peran Perangkat

| Perangkat | Peran | Alasan |
|---|---|---|
| **ESP32** (banyak node, tersebar di lahan) | Baca sensor lokal (soil moisture, suhu, kelembapan udara) + kontrol solenoid valve zonanya | Spek kecil, cukup untuk tugas sederhana "tangan-kaki"; hemat daya (didukung mini solar panel) |
| **Raspberry Pi 3** (satu unit pusat) | Broker komunikasi, digital twin, prediksi, decision engine, penyimpanan data, dashboard | Otak tunggal sistem — semua logika berat dipusatkan di sini |

**Prinsip kunci:** ESP32 tidak "berpikir" — hanya mengirim data & menjalankan perintah. Semua kecerdasan (simulasi, prediksi, keputusan) ada di Pi 3.

---

## 3. Flow Sistem (High-Level)

```
[ESP32 × N node] --data sensor (JSON)--> [Pi 3: MQTT Broker]
                                                │
                                                ▼
                                    [Rust Core Service]
                                    ├─ Parsing & validasi data
                                    ├─ Update model digital twin (water balance)
                                    ├─ Prediksi kebutuhan air (model ringan)
                                    ├─ Decision engine (aturan/threshold)
                                    └─ Simpan histori (SQLite)
                                                │
                                                ▼
[ESP32 zona terkait] <--perintah (buka/tutup valve, durasi)--
                                                │
                                    [Dashboard lokal via WiFi]
                                    (Rust web server: Axum)
```

---

## 4. Komponen Teknis yang Diusulkan

| Kebutuhan | Pilihan MVP | Catatan |
|---|---|---|
| Komunikasi ESP32 ↔ Pi | MQTT (Mosquitto, sudah matang) | Pakai yang paling gampang diset up dulu, bukan yang paling "Rust native" |
| Runtime async | `tokio` (Rust) | Konfigurasi default dulu, tuning belakangan kalau perlu |
| Dashboard | Skip dulu, atau halaman HTML statis paling sederhana | Prioritas: buktikan data masuk & valve bisa dikontrol, bukan tampilan |
| Database lokal | `rusqlite` (SQLite) | Simpan data mentah dulu, skema seadanya |
| Model "prediksi" | Rule-based sederhana (if-else berdasarkan threshold) | **Bukan ML dulu** — cukup: "kalau kelembapan < X%, buka valve Y detik" |
| GPIO langsung di Pi | Skip, kecuali memang ada sensor nempel langsung di Pi | Fokus ke jalur ESP32 → Pi dulu |

*Opsi yang lebih canggih (`rumqttd` full-Rust, model ML dengan `linfa`, dashboard interaktif) dicatat sebagai kandidat untuk iterasi berikutnya, bukan target MVP ini.*

**Build strategy:** Cross-compile dari laptop/PC (target `aarch64-unknown-linux-gnu`, sesuai Raspberry Pi OS Lite 64-bit), lalu transfer binary jadi ke Pi 3 — hindari compile langsung di Pi 3 (lambat). Cek dulu dengan `uname -m` di Pi — pastikan hasilnya `aarch64`.

---

## 5. Hal yang Perlu Diputuskan Bersama Tim

Ini poin-poin yang sebaiknya didiskusikan sebelum mulai implementasi:

1. **Protokol komunikasi ESP32 ↔ Pi**
   Mosquitto (matang, teruji) vs `rumqttd` (full-Rust, tapi lebih baru) vs raw TCP/UDP (paling ringan, tapi perlu handle reliability sendiri)

2. **Kompleksitas model prediksi**
   Seberapa "pintar" prediksi kebutuhan air perlu? Rule-based sederhana dulu (MVP) atau langsung model statistik/ML ringan?

3. **Frekuensi update & jumlah node**
   Dengan 50 sensor (sesuai desain awal), perlu diuji beban Pi 3 menangani volume data ini secara real-time — apakah perlu throttling/batching?

4. **Fallback jika Pi 3 down**
   Apakah ESP32 punya mode otonom sederhana (misal threshold lokal) saat komunikasi ke Pi terputus?

5. **Update proposal**
   Bagian Metode & Gambar 7 di proposal perlu direvisi agar konsisten dengan arsitektur baru (tanpa cloud, Rust, Pi 3) — mempengaruhi narasi "Digital Twin & Simulation Layer" dan disebutkan di bagian TKT.

6. **Dashboard: perlu diakses dari luar jaringan lokal atau cukup WiFi lokal saja?**
   Ini menentukan apakah perlu port forwarding/VPN atau cukup akses dalam satu jaringan.

---

## 6. Roadmap Kerja — 4 Fase

### FASE A — Buktikan konsep manual (belum ada CI/CD)
1. Setup Pi 3 (OS Lite 64-bit, SSH aktif, cek `uname -m` = `aarch64`)
2. Project Rust dasar di laptop, jalan normal (`cargo build` biasa)
3. Cross-compile manual (`cross build --release --target aarch64-unknown-linux-gnu`)
4. Transfer manual (`scp`) ke Pi, jalankan, pastikan bisa
5. Setup Mosquitto di Pi (`apt install mosquitto`)
6. 1 ESP32 dummy → kirim data → Pi terima (test MQTT, bisa pakai `mosquitto_pub` dari laptop dulu tanpa ESP32 fisik)
7. Tambah rule-based logic → Pi kirim perintah balik
8. Sambungkan ke ESP32 asli + sensor + valve fisik
9. Uji end-to-end: sensor baca → Pi putuskan → valve gerak

### FASE B — Otomatisasi build (CI/CD, versi awal)
10. Push project ke GitHub atau GitLab (pilih salah satu)
11. Buat pipeline: build & cross-compile otomatis (belum ada deploy dulu)
12. Pastikan artifact hasil CI = hasil build manual (bandingkan langsung)

### FASE C — Otomatisasi deploy (opsional, setelah Fase A–B stabil)
13. Putuskan: self-hosted runner di jaringan lokal Pi, atau manual pull binary di Pi
14. Setup SSH key khusus CI (bukan key pribadi)
15. Tambah step auto-deploy + restart service (systemd) di pipeline

### FASE D — Scale up (setelah MVP terbukti jalan)
16. Tambah node ESP32 lebih banyak (menuju 50 sensor sesuai desain awal proposal)
17. Pertimbangkan model prediksi lebih baik (dari rule-based → statistik/ML ringan)
18. Dashboard lebih lengkap
19. Update dokumen proposal (bagian Metode & Gambar 7) sesuai arsitektur final

**Prinsip: jangan bangun semua komponen sekaligus. Buktikan 1 jalur data end-to-end dulu (Fase A), baru diperluas.**

---

## 7. Rencana CI/CD (Detail Fase B & C)

### Target compile
```
aarch64-unknown-linux-gnu
```
(karena Raspberry Pi OS Lite 64-bit)

### Contoh pipeline — GitHub Actions (`.github/workflows/build.yml`)
```yaml
name: Build for Raspberry Pi 3 (64-bit Lite)

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-unknown-linux-gnu

      - name: Install cross
        run: cargo install cross --git https://github.com/cross-rs/cross

      - name: Build (cross-compile)
        run: cross build --release --target aarch64-unknown-linux-gnu

      - name: Upload binary artifact
        uses: actions/upload-artifact@v4
        with:
          name: irigasi-binary-aarch64
          path: target/aarch64-unknown-linux-gnu/release/nama-binary-kamu
```

### Contoh pipeline — GitLab CI (`.gitlab-ci.yml`)
```yaml
stages:
  - build
  - deploy

build:
  stage: build
  image: rust:latest
  before_script:
    - rustup target add aarch64-unknown-linux-gnu
    - cargo install cross --git https://github.com/cross-rs/cross
  script:
    - cross build --release --target aarch64-unknown-linux-gnu
  artifacts:
    paths:
      - target/aarch64-unknown-linux-gnu/release/nama-binary-kamu

deploy:
  stage: deploy
  image: alpine:latest
  before_script:
    - apk add --no-cache openssh-client
    - eval $(ssh-agent -s)
    - echo "$PI_SSH_KEY" | tr -d '\r' | ssh-add -
    - mkdir -p ~/.ssh && chmod 700 ~/.ssh
  script:
    - scp -o StrictHostKeyChecking=no target/aarch64-unknown-linux-gnu/release/nama-binary-kamu pi@<IP_PI>:/home/pi/app/
    - ssh -o StrictHostKeyChecking=no pi@<IP_PI> "sudo systemctl restart irigasi-service"
  only:
    - main
```

### Kendala yang perlu diantisipasi: Pi 3 di jaringan lokal (tanpa IP publik)
Pipeline "build" bisa tetap jalan di cloud (GitHub/GitLab runner), tapi step "deploy" butuh akses langsung ke Pi. Dua opsi:

1. **Self-hosted runner** — install runner GitHub/GitLab di jaringan lokal yang sama dengan Pi, supaya step deploy bisa akses Pi tanpa expose ke internet
2. **Manual pull** — pipeline berhenti di tahap upload artifact/release, lalu di Pi jalankan script sederhana (manual atau cron) untuk download binary terbaru:
   ```bash
   curl -L https://github.com/user/repo/releases/latest/download/binary -o app
   chmod +x app
   sudo systemctl restart irigasi-service
   ```

**Rekomendasi untuk MVP:** mulai dari opsi manual pull dulu (lebih sederhana, gak perlu setup SSH access dari CI ke Pi). Auto-deploy via SSH baru dipertimbangkan kalau frekuensi push sudah tinggi dan proses manual mulai terasa merepotkan.

---

*Dokumen ini adalah draft awal untuk bahan diskusi tim, bukan spesifikasi final.*