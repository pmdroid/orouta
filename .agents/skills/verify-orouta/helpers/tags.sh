#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
META="$ROOT/.agents/skills/verify-orouta/current/meta.json"
DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)
      DIR="$2"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$DIR" ]]; then
  echo "--dir required" >&2
  exit 2
fi

"$ROOT/.agents/skills/verify-orouta/helpers/doctor.sh" "$META" >/dev/null

python3 - "$META" "$DIR" <<'PY'
import json, os, sys, urllib.request

meta = json.load(open(sys.argv[1]))
out = sys.argv[2]
os.makedirs(out, exist_ok=True)
base = meta["url"].rstrip("/")
key = meta["key"]
headers = {"Authorization": f"Bearer {key}"}

def get(path):
    req = urllib.request.Request(base + path, headers=headers)
    with urllib.request.urlopen(req, timeout=5) as resp:
        body = resp.read()
        return resp.status, json.loads(body), body

st, tags, raw_tags = get("/api/tags")
st2, models, raw_models = get("/v1/models")
open(os.path.join(out, "tags.json"), "wb").write(raw_tags + b"\n")
open(os.path.join(out, "models.json"), "wb").write(raw_models + b"\n")
open(os.path.join(out, "request.json"), "w").write(json.dumps({
    "url": base,
    "paths": ["/api/tags", "/v1/models"],
    "auth": "Authorization: Bearer",
}, indent=2) + "\n")

tag_names = {m.get("name") for m in tags.get("models", [])}
ids = {m.get("id") for m in models.get("data", [])}
need = {"llama3", "claude-sonnet"}
ok = st == 200 and st2 == 200 and need <= tag_names and need <= ids
result = {
    "ok": ok,
    "tags_status": st,
    "models_status": st2,
    "tag_names": sorted(tag_names),
    "model_ids": sorted(ids),
}
open(os.path.join(out, "result.json"), "w").write(json.dumps(result, indent=2) + "\n")
print(json.dumps(result))
sys.exit(0 if ok else 1)
PY
