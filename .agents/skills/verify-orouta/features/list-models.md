# List models

A client asks orouta which models exist. The answer is the TOML `[[model]]` list, not a live Ollama `/api/tags`.

## Sub-features

- `tags-ollama` returns `{models:[{name,model}]}` for every configured name.
- `tags-openai` returns `{object:list,data:[{id,object:model,owned_by:orouta}]}`.
- `tags-one` `GET /v1/models/{id}` is 200 for a configured id and 404 otherwise.
- `tags-no-upstream` neither list calls the default upstream.

## How to get to it (user POV)

- `curl "$URL/api/tags"` with Bearer or `x-api-key`.
- `curl "$URL/v1/models"` the same way (OpenAI SDK `models.list`).
- `curl "$URL/v1/models/llama3"` for one id.

## Driving it with curl

Preconditions:

- Instance healthy (`helpers/doctor.sh`).
- `meta.json` has `url` and `key`.
- Config includes `llama3` and `claude-sonnet`.

- **Ollama tags.** Run `helpers/tags.sh --dir evidence/run-tags` or `curl -sS -H "Authorization: Bearer $KEY" "$URL/api/tags"`. Status 200. `models[].name` contains `llama3` and `claude-sonnet`.
- **OpenAI list.** `curl -sS -H "Authorization: Bearer $KEY" "$URL/v1/models"`. Status 200. `data[].id` contains the same names. `owned_by` is `orouta`.
- **One model.** `curl -sS -H "Authorization: Bearer $KEY" "$URL/v1/models/llama3"`. Status 200, `id` is `llama3`.
- **Missing model.** `curl -sS -H "Authorization: Bearer $KEY" "$URL/v1/models/does-not-exist"`. Status 404, body `{"error":"unknown model"}`.
- **Proof.** Save both list bodies under `evidence/<run>/tags.json` and `models.json`. `helpers/tags.sh` does this.

## Gotchas

- Lists are synthetic. A dummy default upstream (`127.0.0.1:1`) is fine; a 502 here means you hit the wrong path.
- Names are exact config keys, not whatever a real Ollama has pulled.
- Empty `auth.keys` in a hand-written config skips 401; the verify `up.sh` config always has a key.
