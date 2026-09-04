---
title: API
description: Ollama, OpenAI-compat, and Anthropic Messages.
---

Point clients at orouta the same way you would point them at Ollama.

## Ollama

`POST /api/chat`, `/api/generate`, `/api/embed`, `/api/pull`, and the rest of the Ollama HTTP API are forwarded to the host for `model` or `name`. Streaming is passed through.

Unknown names return `404` `{"error":"unknown model"}`.

## OpenAI

Ollama already serves `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, and `/v1/models`. orouta forwards those paths. Set `base_url` to `http://127.0.0.1:11434/v1` and the API key to a value from `auth.keys`.

## Anthropic Messages

`POST /v1/messages` is translated to Ollama `POST /api/chat`. Text only. `max_tokens` becomes `options.num_predict`. Tools, images, and non-text blocks return `400`.

```sh
curl -H "x-api-key: sk-orouta-alice" \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3","max_tokens":64,"messages":[{"role":"user","content":"hi"}]}' \
  http://127.0.0.1:11434/v1/messages
```

The JSON `model` field still picks the Ollama host. There is no Anthropic upstream.
