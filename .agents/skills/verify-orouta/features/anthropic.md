# Anthropic messages

`POST /v1/messages` is a client dialect. orouta translates it to Ollama `POST /api/chat` and maps the response (JSON or SSE) back. Upstream is still Ollama.

## Sub-features

- `msg-text` text `messages` + `system` become Ollama chat messages; `max_tokens` is `options.num_predict`.
- `msg-echo-model` response `model` is the client name, not `upstream_model`.
- `msg-stream` Ollama NDJSON becomes Anthropic SSE (`content_block_delta`, `message_stop`).
- `msg-reject` tools or non-text content blocks are 400 with no upstream call.

## How to get to it (user POV)

- Anthropic SDK `base_url=$URL` and `api_key=$KEY` (`x-api-key`).
- `curl` `POST /v1/messages`.

## Driving it with curl

Preconditions:

- `up.sh --home-url <ollama-or-mock>` with `/api/chat`.
- A configured model name (verify config: `claude-sonnet` → `llama3` via `upstream_model` only if set; default verify maps both to home as-is).
- `doctor.sh` passed.

- **Non-stream.** `curl -sS -H "x-api-key: $KEY" -H "Content-Type: application/json" -d '{"model":"llama3","max_tokens":64,"system":"be brief","messages":[{"role":"user","content":"hi"}]}' "$URL/v1/messages"`. Status 200. `content[0].text` is the assistant text. `model` is `llama3`. Upstream path is `/api/chat`.
- **Reject tools.** Same URL with `"tools":[{"name":"x"}]`. Status 400. No upstream call.
- **Stream.** `"stream":true`. `Content-Type` is `text/event-stream`. Body contains `event: content_block_delta` and `event: message_stop`.
- **Proof.** Save the Anthropic request, the translated `/api/chat` body the mock saw, and the client response.

## Gotchas

- Images, documents, `tool_choice` are 400 in v1.
- `anthropic-version` header is ignored.
- Unknown `model` is 404 before upstream, same as other inference paths.
