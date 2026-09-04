---
name: verify-orouta
description: Drive the real orouta HTTP proxy the way a client does — launch a throwaway instance, curl Ollama/OpenAI/Anthropic routes, capture response bodies. Use for verifying orouta features, pre-PR API checks, or /verify-orouta.
---

# Verify orouta

orouta is an HTTP proxy. Clients speak Ollama `/api/*`, OpenAI-compat `/v1/chat/completions`, and Anthropic `POST /v1/messages`. Every run uses a **throwaway instance**: temp config, `127.0.0.1` + a free port, never `11434`. Parallel runs are safe. The doctor refuses port `11434` (user's real bind).

## Launch

From the repo root:

```sh
.agents/skills/verify-orouta/helpers/up.sh
```

Builds `target/debug/orouta`, writes a temp TOML (auth key `sk-orouta-verify`, models `llama3` and `claude-sonnet`, dummy default upstream `http://127.0.0.1:1`), binds `127.0.0.1:<free-port>`, waits until `GET /api/tags` with the key returns 200. Prints **one JSON line on stdout** (logs on stderr). Same JSON at `.agents/skills/verify-orouta/current/meta.json`.

```json
{"name":"verify-orouta","url":"http://127.0.0.1:50190","port":50190,"pid":1234,"config":"/var/folders/…/orouta.toml","logFile":"…/server.log","key":"sk-orouta-verify","workdir":"/var/folders/…"}
```

Ready means that line exists and `doctor.sh` passes. Teardown is `helpers/down.sh`.

## Doctor

```sh
.agents/skills/verify-orouta/helpers/doctor.sh
```

Reads `current/meta.json`. Checks, in order: port is not `11434` → pid is alive → `GET /api/tags` with Bearer key is 200 → body lists `llama3` and `claude-sonnet`. Prints `/api/tags` JSON so you know the build you are driving.

Pass an explicit meta file: `helpers/doctor.sh path/to/meta.json`.

## Drive

Harness is `curl` against `$url` with `Authorization: Bearer $key` (or `x-api-key`). Stable handles are the **paths**, not headers beyond auth:

| Path | What it is |
|---|---|
| `GET /api/tags` | synthetic Ollama tag list from TOML |
| `GET /v1/models` | synthetic OpenAI model list from TOML |
| `GET /v1/models/{id}` | one configured model or 404 |
| `POST /api/chat` | byte-forward to the model's Ollama host |
| `POST /v1/chat/completions` | byte-forward OpenAI-compat |
| `POST /v1/messages` | Anthropic dialect → Ollama `/api/chat` |

List models (no upstream needed):

```sh
.agents/skills/verify-orouta/helpers/tags.sh \
  --dir ".agents/skills/verify-orouta/evidence/run-tags"
```

Reads `current/meta.json`. Writes request/response JSON under `--dir`. Exit 0 only if both lists contain `llama3` and `claude-sonnet`.

Auth:

```sh
curl -sS -o /dev/null -w "%{http_code}\n" "$URL/api/tags"
curl -sS -H "Authorization: Bearer $KEY" "$URL/api/tags"
curl -sS -H "x-api-key: $KEY" "$URL/api/tags"
```

Chat/Anthropic need a live or mocked Ollama at the upstream `base_url`. The default verify config points at `127.0.0.1:1` so those routes 502 unless you pass `--home-url` to `up.sh`.

```sh
.agents/skills/verify-orouta/helpers/up.sh --home-url http://127.0.0.1:11435
```

## Evidence

Capture into `.agents/skills/verify-orouta/evidence/<run-name>/` (gitignored, survives teardown):

1. **Action** — the curl (method, path, headers, body) saved next to the response.
2. **State** — status + JSON body from the same client path a user would hit.
3. **Side effects** — for proxy routes, the mock/upstream access log or a second `GET` that shows routing (home vs desk). Do not treat `cargo test` as the user path.

Proof standard: hit the running binary over HTTP. In-process `orouta::app` tests are not this skill.

## Cleanup

```sh
.agents/skills/verify-orouta/helpers/down.sh
```

Reads `current/meta.json`, SIGTERM then SIGKILL that pid, removes `workdir` and `current/`. Never `pkill orouta`. Run after every attempt, failed included. `evidence/` stays.

## Helpers

| Script | Invocation | Purpose |
|---|---|---|
| `up.sh` | `helpers/up.sh [--home-url URL] [--name TAG]` | build, launch, write `current/meta.json` |
| `doctor.sh` | `helpers/doctor.sh [meta.json]` | read-only worth-driving check |
| `tags.sh` | `helpers/tags.sh --dir EVIDENCE_DIR` | `GET /api/tags` + `GET /v1/models` with assertions |
| `down.sh` | `helpers/down.sh` | stop the pid this run started |
