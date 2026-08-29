#!/bin/bash
# Dijalankan di Raspberry Pi (via SSH), bukan di laptop dev.
# Broker MQTT pakai broker.emqx.io (public), jadi tidak perlu install Mosquitto lokal --
# cukup siapkan direktori app. Lihat plan.md §5 Fase A step 7.
set -euo pipefail

echo "== cek arsitektur (harus aarch64) =="
uname -m

mkdir -p /home/irigasi/app/data

echo "== selesai. /home/irigasi/app siap untuk terima binary =="
