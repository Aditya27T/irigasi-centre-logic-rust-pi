#!/bin/bash
# Dijalankan di Raspberry Pi (via SSH), SEKALI SAJA saat Pi baru diinstall.
# Asumsi: seluruh isi folder deploy/ (script ini + irigasi.service + config.pi.toml)
# sudah di-scp apa adanya ke Pi, dan dijalankan dari folder itu. Setelah script ini
# selesai, jalankan deploy.sh di folder yang sama untuk ambil+jalankan binary dari
# GitLab CI -- lihat plan.md §5 Fase A step 7-8 & §7 Fase C.
set -euo pipefail

echo "== cek arsitektur (harus aarch64) =="
uname -m

echo "== install dependency OS (mosquitto lokal + tools ambil artifact) =="
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends mosquitto mosquitto-clients unzip curl
sudo systemctl enable --now mosquitto.service

echo "== siapkan direktori app =="
mkdir -p /home/irigasi/app/data

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -f "$SCRIPT_DIR/config.pi.toml" ] && [ ! -f /home/irigasi/app/config.toml ]; then
  cp "$SCRIPT_DIR/config.pi.toml" /home/irigasi/app/config.toml
  echo "config.toml terpasang dari config.pi.toml"
elif [ -f /home/irigasi/app/config.toml ]; then
  echo "config.toml sudah ada di /home/irigasi/app -- tidak ditimpa (edit manual kalau perlu update)"
else
  echo "PERINGATAN: config.pi.toml tidak ditemukan di $SCRIPT_DIR -- salin manual ke /home/irigasi/app/config.toml sebelum start service" >&2
fi

if [ -f "$SCRIPT_DIR/irigasi.service" ]; then
  sudo cp "$SCRIPT_DIR/irigasi.service" /etc/systemd/system/irigasi.service
  sudo systemctl daemon-reload
  sudo systemctl enable irigasi.service
  echo "irigasi.service terpasang & enabled (belum di-start -- binary belum ada)"
else
  echo "PERINGATAN: irigasi.service tidak ditemukan di $SCRIPT_DIR" >&2
fi

echo ""
echo "== selesai setup dasar. Langkah selanjutnya =="
echo "1. Buat GitLab Project Access Token (scope read_api) di GitLab > Settings > Access Tokens"
echo "2. echo '<token>' > ~/.irigasi-deploy-token && chmod 600 ~/.irigasi-deploy-token"
echo "3. Jalankan: $SCRIPT_DIR/deploy.sh   (ambil binary terbaru dari CI + start service)"
