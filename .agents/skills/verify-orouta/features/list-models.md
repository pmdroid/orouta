# List models

A client asks orouta which models exist. orouta asks each configured host `GET /api/tags` and unions the names.

## Sub-features

- `tags-ollama` returns `{models:[...]}` from the hosts.
- `tags-openai` returns `{object:list,data:[{id,object:model,owned_by:orouta}]}` with the same names.
- `tags-one` `GET /v1/models/{id}` is 200 if any host listed that id (including `llama3` for `llama3:latest`) and 404 otherwise.
- `tags-down` a host that does not answer is skipped.

## How to get to it (user POV)

- `curl "$URL/api/tags"` with Bearer or `x-api-key`.
- `curl "$URL/v1/models"` the same way (OpenAI SDK `models.list`).
- `curl "$URL/v1/models/llama3"` for one id.

## Driving it with curl

Preconditions:

- Instance healthy (`helpers/doctor.sh`).
- `meta.json` has `url` and `key`.

- **Ollama tags.** Run `helpers/tags.sh --dir evidence/run-tags` or `curl -sS -H "Authorization: Bearer $KEY" "$URL/api/tags"`. Status 200. Body is a models array (empty if no host answered).
- **OpenAI list.** `curl -sS -H "Authorization: Bearer $KEY" "$URL/v1/models"`. Status 200. `owned_by` is `orouta` when data is non-empty.
- **Missing model.** `curl -sS -H "Authorization: Bearer $KEY" "$URL/v1/models/does-not-exist"`. Status 404, body `{"error":"unknown model"}`.
- **Proof.** Save both list bodies under `evidence/<run>/tags.json` and `models.json`. `helpers/tags.sh` does this.

## Gotchas

- Lists are live. A dummy default upstream (`127.0.0.1:1`) yields an empty list after a short timeout, not a 502 on `/api/tags`.
- A name should exist on only one host. If two hosts list the same name, the first in config order wins.
- Empty `auth.keys` in a hand-written config skips 401; the verify `up.sh` config always has a key.
