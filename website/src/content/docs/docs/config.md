---
title: Config
description: TOML config for hosts and API keys.
---

Copy the example and edit it:

```sh
cp orouta.toml.example orouta.toml
orouta --config orouta.toml
```

`--config` defaults to `orouta.toml` in the working directory. Default bind is `0.0.0.0:11434`.

```toml
bind = "0.0.0.0:11434"

[auth]
keys = ["sk-orouta-alice"]

[[upstream]]
id = "home"
base_url = "http://192.168.1.10:11434"
api_key = ""
default = true

[[upstream]]
id = "desk"
base_url = "http://192.168.1.20:11434"
```

List hosts only. orouta calls `GET /api/tags` on each one and builds the model list. Put a given model name on one host. Clients can send `llama3` when the host reports `llama3:latest`.

`auth.keys` empty means no client auth. Otherwise send `Authorization: Bearer` or `x-api-key`.

Exactly one upstream must have `default = true`. That host gets management calls with an unknown name, plus paths that have no model (`/api/ps`, `/api/version`, blobs).

An upstream `api_key` is sent to that Ollama as `Authorization: Bearer`. The client's key is stripped.

`GET /api/tags` and `GET /v1/models` are the union of what the hosts report.
