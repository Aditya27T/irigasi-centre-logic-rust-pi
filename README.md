# Irigasi Digital Twin (irigasi-core)

Sistem digital twin irigasi presisi, **edge-only** (tanpa cloud): ESP32 di lahan membaca sensor tanah/suhu dan mengeksekusi valve, Raspberry Pi 3 menjalankan broker MQTT + decision engine + dashboard, semua dalam satu binary Rust (`irigasi-core`).

```
[ESP32 × N node] --sensor (MQTT JSON)--> [Pi: broker + irigasi-core]
                                                │
                                     decision engine (threshold/jadwal)
                                                │
       [ESP32] <--command (open/close valve)---┘
                                                │
                                     [Dashboard web :8080]
```

- **ESP32** ([iot.c](iot.c)): baca 2 pasang sensor (soil+suhu), publish tiap ~2 detik, eksekusi command buka/tutup 2 valve independen. ESP32 tidak mengambil keputusan sendiri — murni "tangan-kaki".
- **Pi / `irigasi-core`** (Rust): subscribe sensor, jalankan decision engine (mode `sensor` threshold atau `schedule` jam siram), publish command, simpan histori ke SQLite, serve dashboard web (opsional, `--features dashboard`).

Latar belakang & rancangan lengkap ada di [brief.md](brief.md) (spesifikasi awal tim) dan [plan.md](plan.md) (log implementasi per fase, termasuk keputusan & bugfix yang sudah dilakukan).

## Struktur repo

```
irigasi/
├── src/                  # source Rust (irigasi-core)
├── iot.c                 # firmware ESP32 (Arduino)
├── config/default.toml   # config untuk dev lokal (laptop)
├── deploy/
│   ├── config.pi.toml    # config untuk Raspberry Pi (broker, node, threshold)
│   ├── irigasi.service   # unit systemd
│   ├── pi-setup.sh       # bootstrap sekali di Pi baru
│   ├── deploy.sh         # ambil binary terbaru dari GitLab CI + restart service
│   └── .ssh/             # (gitignored) key SSH khusus deploy, generate sendiri kalau perlu
├── .gitlab-ci.yml        # CI: cross-compile aarch64 tiap push ke main/MR
└── plan.md               # dokumentasi desain & histori perubahan
```

## Development lokal

Butuh Rust stable (`rustup`) dan (opsional) Mosquitto lokal untuk uji coba tanpa hardware:

```bash
brew install mosquitto   # macOS, kalau mau tes tanpa broker cloud/Pi
cargo build                              # build biasa
cargo build --features dashboard         # sertakan dashboard web
cargo test                               # unit test decision engine, dll.
cargo run --features dashboard           # jalan pakai config/default.toml
```

Dashboard (kalau `--features dashboard` & `[dashboard].enabled = true` di config) akan hidup di `http://localhost:8080/`.

## Deploy ke Raspberry Pi

CI GitLab (`.gitlab-ci.yml`) otomatis cross-compile binary `aarch64-unknown-linux-gnu` (dengan `--features dashboard`) setiap push ke `main`. Untuk memasangnya ke Pi, ikuti salah satu:

### A. Pi yang sudah pernah di-setup (update rutin)

Cukup jalankan `deploy.sh` yang sudah ada di Pi:

```bash
ssh irigasi@<ip-pi>
cd ~/deploy-staging   # atau lokasi lain tempat deploy.sh disimpan
./deploy.sh
```

Script ini otomatis ambil artifact terbaru dari branch `main`, backup binary lama, pasang binary baru, dan restart `irigasi.service`.

### B. Pi baru diinstall dari nol

1. **Flash & boot Pi** — pakai Raspberry Pi Imager, aktifkan SSH + WiFi saat flashing (atau `raspi-config` manual). Pastikan ada user `irigasi` (sesuai `deploy/irigasi.service`, `User=irigasi`).

2. **(Opsional) pasang SSH key deploy** supaya langkah berikutnya tidak perlu ketik password tiap kali. Kalau belum ada key-nya, generate dulu di laptop:
   ```bash
   ssh-keygen -t ed25519 -f deploy/.ssh/irigasi_pi -C irigasi-deploy -N ""
   ```
   Lalu pasang ke Pi:
   ```bash
   ssh-copy-id -i deploy/.ssh/irigasi_pi.pub irigasi@<ip-pi>
   ```
   (Folder `deploy/.ssh/` sudah di-`.gitignore`, jadi private key ini **tidak ikut ter-clone** — tiap orang generate sendiri atau minta dikirim terpisah, bukan lewat git.)

3. **Clone repo ini** (kalau belum) lalu salin folder `deploy/` ke Pi:
   ```bash
   git clone git@gitlab.com:adityaeka15/irigasi-digtwin.git
   cd irigasi-digtwin
   scp -r deploy irigasi@<ip-pi>:~/deploy-staging
   ```

4. **Jalankan setup dasar** di Pi:
   ```bash
   ssh irigasi@<ip-pi>
   cd ~/deploy-staging
   ./pi-setup.sh
   ```
   Ini akan: install `mosquitto` + `mosquitto-clients` + `unzip` + `curl`, siapkan `/home/irigasi/app/{,data}`, pasang `config.toml` (dari `config.pi.toml` — cek dulu isinya, terutama broker MQTT & threshold node, sesuaikan kalau beda dari setup Anda), serta pasang & enable `irigasi.service` ke systemd (belum start — binary belum ada).

5. **Buat GitLab Project Access Token** (sekali per Pi, harus manual lewat web GitLab — tidak bisa di-CLI-kan): buka project ini di GitLab → **Settings → Access Tokens** → buat token baru dengan scope **`read_api`**, role minimal **Reporter**. Simpan tokennya di Pi:
   ```bash
   echo '<token-dari-gitlab>' > ~/.irigasi-deploy-token
   chmod 600 ~/.irigasi-deploy-token
   ```

6. **Ambil & jalankan binary pertama kali**:
   ```bash
   ~/deploy-staging/deploy.sh
   ```
   Kalau sukses, script akan print versi/git-SHA binary yang barusan terpasang dan konfirmasi `irigasi.service` aktif.

7. **Verifikasi**:
   ```bash
   systemctl status irigasi.service
   journalctl -u irigasi.service -f     # harus ada log "Terhubung ke MQTT broker..." & "Dashboard HTTP aktif..."
   ```
   Buka `http://<ip-pi>:8080/` dari browser (harus 1 jaringan WiFi/LAN yang sama dengan Pi & ESP32).

8. **Update selanjutnya** cukup ulangi langkah 6 (`./deploy.sh`) setiap habis pipeline GitLab sukses — tidak perlu ulang langkah 1–5.

### Catatan penting

- **ESP32 & Pi wajib satu jaringan WiFi** yang sama dengan yang di-hardcode di [iot.c](iot.c) (`ssid`/`password`) — kalau ganti WiFi, firmware ESP32 perlu di-flash ulang dengan SSID baru.
- **IP Pi bisa berubah** (DHCP) — cek ulang lewat `hostname -I` di Pi atau daftar client di router kalau dashboard tiba-tiba tidak bisa diakses.
- `deploy/config.pi.toml` berisi kredensial broker MQTT — hati-hati kalau mau ganti ke broker lain, jangan commit kredensial baru tanpa didiskusikan dulu ke tim.
