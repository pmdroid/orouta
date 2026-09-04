# Chat proxy

`model` in the JSON body selects the Ollama host. `/api/chat` and `/v1/chat/completions` are byte-forwarded. Unknown inference models are 404 and never hit upstream.

## Sub-features

- `chat-route` `POST /api/chat` with `llama3` hits the home upstream only.
- `chat-openai` `POST /v1/chat/completions` is the same host, OpenAI path.
- `chat-unknown` unknown `model` on inference paths is 404, no upstream call.
- `chat-stream` streamed upstream bytes reach the client.

## How to get to it (user POV)

- Ollama CLI/SDK against `OLLAMA_HOST=$URL` with `model` set to a configured name.
- OpenAI SDK `base_url=$URL/v1` and `api_key=$KEY`.

## Driving it with curl

Preconditions:

- `up.sh --home-url <mock-or-real-ollama>` so `/api/chat` has somewhere to go.
- `doctor.sh` passed.
- Home serves Ollama `/api/chat` and `/v1/chat/completions`.

- **Ollama chat.** `curl -sS -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" -d '{"model":"llama3","messages":[{"role":"user","content":"hi"}],"stream":false}' "$URL/api/chat"`. Status 200 from upstream. Home received the body; desk did not.
- **OpenAI.** Same JSON to `"$URL/v1/chat/completions"`. Upstream `Authorization` is the config `api_key`, not `$KEY`.
- **Unknown.** `{"model":"does-not-exist",...}` to `/api/chat`. Status 404 `{"error":"unknown model"}`. Upstream not called.
- **Proof.** Save client request/response and the upstream mock's recorded request.

## Gotchas

- Default `up.sh` home is `http://127.0.0.1:1`. Chat will fail until `--home-url`.
- Path is forwarded as-is. `/v1/messages` is the Anthropic feature, not this one.
- `upstream_model` rewrites JSON `model`/`name` only when those keys already exist.
