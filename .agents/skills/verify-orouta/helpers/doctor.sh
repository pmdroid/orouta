#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
META="${1:-$ROOT/.agents/skills/verify-orouta/current/meta.json}"

if [[ ! -f "$META" ]]; then
  echo "missing meta: $META" >&2
  exit 1
fi

python3 - "$META" <<'PY'
import json, os, sys, urllib.request

meta = json.load(open(sys.argv[1]))
port = int(meta["port"])
if port == 11434:
    sys.stderr.write("refusing port 11434\n")
    sys.exit(1)
pid = int(meta["pid"])
try:
    os.kill(pid, 0)
except OSError:
    sys.stderr.write(f"pid {pid} not running\n")
    sys.exit(1)

url = meta["url"].rstrip("/") + "/api/tags"
req = urllib.request.Request(url, headers={"Authorization": f"Bearer {meta['key']}"})
try:
    with urllib.request.urlopen(req, timeout=5) as resp:
        body = resp.read()
        status = resp.status
except Exception as e:
    sys.stderr.write(f"GET /api/tags failed: {e}\n")
    sys.exit(1)

if status != 200:
    sys.stderr.write(f"GET /api/tags status {status}\n")
    sys.exit(1)
data = json.loads(body)
names = {m.get("name") for m in data.get("models", [])}
need = {"llama3", "claude-sonnet"}
if not need <= names:
    sys.stderr.write(f"models {names} missing {need - names}\n")
    sys.exit(1)
sys.stdout.write(body.decode() + "\n")
PY
