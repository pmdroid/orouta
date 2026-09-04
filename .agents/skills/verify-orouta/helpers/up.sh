#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
SKILL="$ROOT/.agents/skills/verify-orouta"
NAME="verify-orouta"
HOME_URL="http://127.0.0.1:1"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --home-url)
      HOME_URL="$2"
      shift 2
      ;;
    --name)
      NAME="$2"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')
if [[ "$PORT" == "11434" ]]; then
  echo "refusing port 11434" >&2
  exit 1
fi

WORKDIR=$(mktemp -d -t orouta-verify.XXXXXX)
CONFIG="$WORKDIR/orouta.toml"
LOG="$WORKDIR/server.log"
KEY="sk-orouta-verify"

cat > "$CONFIG" <<EOF
bind = "127.0.0.1:${PORT}"

[auth]
keys = ["${KEY}"]

[[upstream]]
id = "home"
base_url = "${HOME_URL}"
api_key = ""
default = true
EOF

cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/debug/orouta"
"$BIN" --config "$CONFIG" >"$LOG" 2>&1 &
PID=$!

cleanup_fail() {
  kill "$PID" 2>/dev/null || true
  wait "$PID" 2>/dev/null || true
}

ready=0
for _ in $(seq 1 80); do
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "server died" >&2
    cat "$LOG" >&2
    exit 1
  fi
  code=$(curl -s -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${KEY}" "http://127.0.0.1:${PORT}/api/tags" || true)
  if [[ "$code" == "200" ]]; then
    ready=1
    break
  fi
  sleep 0.1
done

if [[ "$ready" != "1" ]]; then
  echo "not ready" >&2
  cat "$LOG" >&2
  cleanup_fail
  exit 1
fi

mkdir -p "$SKILL/current"
META=$(python3 -c "import json; print(json.dumps({
  'name': '''$NAME''',
  'url': 'http://127.0.0.1:$PORT',
  'port': int('$PORT'),
  'pid': int('$PID'),
  'config': '''$CONFIG''',
  'logFile': '''$LOG''',
  'key': '''$KEY''',
  'workdir': '''$WORKDIR''',
}))")
printf '%s\n' "$META" | tee "$SKILL/current/meta.json"
