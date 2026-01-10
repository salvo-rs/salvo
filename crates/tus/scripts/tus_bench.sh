#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${1:-http://127.0.0.1:5800/files}"
HEAD_COUNT="${HEAD_COUNT:-200}"
HEAD_CONCURRENCY="${HEAD_CONCURRENCY:-50}"
HEAD_COUNT_BASE="${HEAD_COUNT_BASE:-$HEAD_COUNT}"
HEAD_CONCURRENCY_BASE="${HEAD_CONCURRENCY_BASE:-1}"
PATCH_COUNT="${PATCH_COUNT:-50}"
PATCH_CONCURRENCY="${PATCH_CONCURRENCY:-50}"
PATCH_COUNT_BASE="${PATCH_COUNT_BASE:-$PATCH_COUNT}"
PATCH_CONCURRENCY_BASE="${PATCH_CONCURRENCY_BASE:-1}"
UPLOAD_LENGTH="${UPLOAD_LENGTH:-1024}"
CURL_CONNECT_TIMEOUT="${CURL_CONNECT_TIMEOUT:-2}"
CURL_MAX_TIME="${CURL_MAX_TIME:-10}"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

chunk="$tmpdir/chunk.bin"
head -c "$UPLOAD_LENGTH" /dev/zero > "$chunk"

create_upload() {
  echo "== Create upload ==" >&2
  local headers="$tmpdir/headers.txt"
  curl -sS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" \
    -D "$headers" -o /dev/null -X POST "$BASE_URL" \
    -H "Tus-Resumable: 1.0.0" \
    -H "Upload-Length: ${UPLOAD_LENGTH}"

  local location
  location="$(awk -F': ' 'tolower($1)=="location"{print $2}' "$headers" | tail -n1 | tr -d '\r')"
  if [[ -z "$location" ]]; then
    echo "Missing Location header from POST" >&2
    exit 1
  fi

  if [[ "$location" = /* ]]; then
    local base_host
    base_host="$(echo "$BASE_URL" | sed -E 's#(https?://[^/]+).*#\1#')"
    echo "${base_host}${location}"
  else
    echo "$location"
  fi
}

head_storm() {
  local label="$1"
  local url="$2"
  local count="$3"
  local concurrency="$4"
  local out="$tmpdir/head_status_${label}.txt"

  echo "== HEAD storm (${label}) count=${count} concurrency=${concurrency} =="
  time seq 1 "$count" | xargs -P "$concurrency" -I{} \
    curl -sS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" \
    -o /dev/null -w "%{http_code}\n" -I \
    -H "Tus-Resumable: 1.0.0" \
    -H "X-Bench-Seq: {}" \
    "$url" > "$out"

  awk '{counts[$1]++} END {for (c in counts) print c, counts[c]}' "$out" | sort
}

patch_storm() {
  local label="$1"
  local url="$2"
  local count="$3"
  local concurrency="$4"
  local out="$tmpdir/patch_status_${label}.txt"

  echo "== PATCH storm (${label}) count=${count} concurrency=${concurrency} =="
  time seq 1 "$count" | xargs -P "$concurrency" -I{} \
    curl -sS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" \
    -o /dev/null -w "%{http_code}\n" -X PATCH \
    -H "Tus-Resumable: 1.0.0" \
    -H "Upload-Offset: 0" \
    -H "Content-Type: application/offset+octet-stream" \
    -H "X-Bench-Seq: {}" \
    --data-binary @"$chunk" \
    "$url" > "$out"

  awk '{counts[$1]++} END {for (c in counts) print c, counts[c]}' "$out" | sort
}

head_check() {
  local url="$1"
  local final_offset
  final_offset="$(curl -sS --connect-timeout "$CURL_CONNECT_TIMEOUT" --max-time "$CURL_MAX_TIME" \
    -D - -o /dev/null -I \
    -H "Tus-Resumable: 1.0.0" \
    "$url" | awk -F': ' 'tolower($1)=="upload-offset"{print $2}' | tr -d '\r')"

  echo "Upload-Offset: ${final_offset}"
  if [[ -n "$final_offset" && "$final_offset" != "$UPLOAD_LENGTH" ]]; then
    echo "Unexpected final offset: ${final_offset} (expected ${UPLOAD_LENGTH})" >&2
    exit 1
  fi
}

upload_url="$(create_upload)"
echo "Upload URL: $upload_url"

head_storm "baseline" "$upload_url" "$HEAD_COUNT_BASE" "$HEAD_CONCURRENCY_BASE"
head_storm "tuned" "$upload_url" "$HEAD_COUNT" "$HEAD_CONCURRENCY"

upload_url_patch_base="$(create_upload)"
echo "Upload URL (patch baseline): $upload_url_patch_base"
patch_storm "baseline" "$upload_url_patch_base" "$PATCH_COUNT_BASE" "$PATCH_CONCURRENCY_BASE"
echo "== HEAD check (patch baseline) =="
head_check "$upload_url_patch_base"

upload_url_patch="$(create_upload)"
echo "Upload URL (patch tuned): $upload_url_patch"
patch_storm "tuned" "$upload_url_patch" "$PATCH_COUNT" "$PATCH_CONCURRENCY"
echo "== HEAD check (patch tuned) =="
head_check "$upload_url_patch"

echo "Done."
