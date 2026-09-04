#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
META="$ROOT/.agents/skills/verify-orouta/current/meta.json"

if [[ ! -f "$META" ]]; then
  echo "nothing to stop" >&2
  exit 0
fi

PID=$(python3 -c "import json; print(json.load(open('$META'))['pid'])")
WORKDIR=$(python3 -c "import json; print(json.load(open('$META'))['workdir'])")

if kill -0 "$PID" 2>/dev/null; then
  kill -TERM "$PID" 2>/dev/null || true
  for _ in $(seq 1 50); do
    if ! kill -0 "$PID" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if kill -0 "$PID" 2>/dev/null; then
    kill -KILL "$PID" 2>/dev/null || true
  fi
fi

rm -rf "$WORKDIR"
rm -rf "$ROOT/.agents/skills/verify-orouta/current"
