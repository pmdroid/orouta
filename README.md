# orouta

<p align="center">
  <img src="docs/logo.png" alt="orouta" width="280">
</p>

Several Ollama hosts behind one port. The JSON `model` (or `name`) picks the host. Anthropic `POST /v1/messages` is translated to Ollama `/api/chat`; everything else is byte-forwarded.

## Run

```
cp orouta.toml.example orouta.toml
cargo run --release -- --config orouta.toml
```

Default bind is `0.0.0.0:11434`. `--config` defaults to `orouta.toml`.

## Config

See `orouta.toml.example`. Logo: `docs/logo.png`. Favicons: `docs/favicons/`.

- `[[model]]` names are what clients send. `upstream` selects an Ollama host. `upstream_model` rewrites `model`/`name` when those keys already exist.
- Exactly one `[[upstream]]` must have `default = true`.
- `auth.keys` empty: no client auth. Non-empty: `Authorization: Bearer` or `x-api-key` must match.
- Upstream `api_key`, if set, is sent as `Authorization: Bearer` to that host. Client auth headers are stripped.

## Examples

```
curl -H "Authorization: Bearer sk-orouta-alice" http://127.0.0.1:11434/api/tags

curl -H "Authorization: Bearer sk-orouta-alice" \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3","messages":[{"role":"user","content":"hi"}]}' \
  http://127.0.0.1:11434/api/chat

curl -H "x-api-key: sk-orouta-alice" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet","max_tokens":64,"system":"be brief","messages":[{"role":"user","content":"hi"}]}' \
  http://127.0.0.1:11434/v1/messages
```
