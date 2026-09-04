# orouta verification map

Maintained source for verifying user-facing HTTP behavior. Read this index, then the matching feature file.

## Baseline preconditions

- Launch via `.agents/skills/verify-orouta/helpers/up.sh` from the repo root.
- Instance binds `127.0.0.1` on a free port. Never drive port `11434`.
- Auth key is `sk-orouta-verify` unless `meta.json` says otherwise.
- Seeded models: `llama3`, `claude-sonnet`.
- `helpers/doctor.sh` must pass before any drive.
- Never drive an instance this run did not start.

## Driving conventions

- Start every recipe from the baseline unless its preconditions say otherwise.
- Paths and header names are literal.
- Use `curl` (or the helper that wraps it). Do not call `orouta::app` in-process.
- Restore nothing: the instance is disposable. Do not delete proof artifacts.

## Proof and skip reporting

- Capture the request and the response body, not only the status code.
- Mutation/proxy proof includes which upstream was hit (mock log or `--home-url`).
- Record the feature ID with every artifact.
- An unreachable path is reported with the unmet precondition, not as verified via another route.

## Feature entry contract

Each feature file: H1 + one paragraph, then exactly four H2s in order: `Sub-features`, `How to get to it (user POV)`, `Driving it with curl`, `Gotchas`.

## Features

- [List models](./list-models.md) — `GET /api/tags` and `GET /v1/models` from config.
- [Auth](./auth.md) — Bearer, `x-api-key`, missing, wrong.
- [Chat proxy](./chat.md) — model name picks the Ollama host; OpenAI `/v1` byte-forward.
- [Anthropic messages](./anthropic.md) — `POST /v1/messages` translated to `/api/chat`.
