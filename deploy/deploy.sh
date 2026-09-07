#!/bin/bash
# Dijalankan di Raspberry Pi (via SSH), bukan di laptop dev.
# Ambil binary hasil build CI GitLab terbaru (branch main, job "build") dan pasang
# jadi /home/irigasi/app/irigasi-core, lalu restart service -- plan.md §7 Fase C step 1.
#
# Syarat sekali di awal (manual, tidak bisa diotomasi dari sini):
#   1. Buat GitLab Project Access Token: Settings > Access Tokens, scope `read_api`,
#      role minimal Reporter -- BUKAN token pribadi akun (prinsip brief.md).
#   2. Simpan di Pi: echo '<token>' > ~/.irigasi-deploy-token && chmod 600 ~/.irigasi-deploy-token
#
# Jalankan manual via SSH setiap habis pipeline CI sukses (plan.md §7 step 2).
set -euo pipefail

APP_DIR="/home/irigasi/app"
TOKEN_FILE="$HOME/.irigasi-deploy-token"
# Project GitLab: git@gitlab.com:adityaeka15/irigasi-digtwin.git -- ganti kalau repo pindah/fork.
PROJECT="adityaeka15%2Firigasi-digtwin"
REF="main"
JOB="build"
ARTIFACT_PATH="target/aarch64-unknown-linux-gnu/release/irigasi-core"

if [ ! -f "$TOKEN_FILE" ]; then
  echo "Token GitLab tidak ditemukan di $TOKEN_FILE" >&2
  echo "Buat Project Access Token (scope: read_api) di GitLab, lalu:" >&2
  echo "  echo '<token>' > $TOKEN_FILE && chmod 600 $TOKEN_FILE" >&2
  exit 1
fi
TOKEN=$(cat "$TOKEN_FILE")

WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

echo "== Mengambil artifact terbaru (branch=$REF, job=$JOB) =="
HTTP_CODE=$(curl --silent --show-error --location --write-out '%{http_code}' \
  --header "PRIVATE-TOKEN: $TOKEN" \
  "https://gitlab.com/api/v4/projects/$PROJECT/jobs/artifacts/$REF/download?job=$JOB" \
  -o "$WORKDIR/artifacts.zip")

if [ "$HTTP_CODE" != "200" ]; then
  echo "Gagal ambil artifact (HTTP $HTTP_CODE) -- cek token, scope, atau apakah job '$JOB' di branch '$REF' pernah sukses." >&2
  exit 1
fi

echo "== Ekstrak binary =="
unzip -o -q "$WORKDIR/artifacts.zip" "$ARTIFACT_PATH" -d "$WORKDIR"

BIN_SRC="$WORKDIR/$ARTIFACT_PATH"
if [ ! -f "$BIN_SRC" ]; then
  echo "Binary tidak ditemukan di artifact (path diharapkan: $ARTIFACT_PATH)" >&2
  exit 1
fi
chmod +x "$BIN_SRC"

NEW_SHA=$("$BIN_SRC" --version 2>/dev/null || echo "tidak diketahui")
echo "Binary baru: $NEW_SHA"

mkdir -p "$APP_DIR/data"
if [ -f "$APP_DIR/irigasi-core" ]; then
  cp "$APP_DIR/irigasi-core" "$APP_DIR/irigasi-core.bak-$(date +%Y%m%d-%H%M%S)"
  # Simpan cuma 3 backup terakhir supaya disk Pi (kapasitas kecil) tidak penuh.
  ls -t "$APP_DIR"/irigasi-core.bak-* 2>/dev/null | tail -n +4 | xargs -r rm -f
fi

install -m 755 "$BIN_SRC" "$APP_DIR/irigasi-core"

echo "== Restart service =="
sudo systemctl restart irigasi.service
sleep 2
systemctl is-active --quiet irigasi.service && echo "irigasi.service aktif ($NEW_SHA)" || {
  echo "irigasi.service GAGAL start -- cek: journalctl -u irigasi.service -n 50" >&2
  exit 1
}
