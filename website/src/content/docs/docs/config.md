---
title: Config
description: TOML config for hosts, models, and API keys.
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

[[model]]
name = "llama3"
upstream = "home"

[[model]]
name = "claude-sonnet"
upstream = "desk"
upstream_model = "llama3:70b"
```

`auth.keys` empty means no client auth. Otherwise send `Authorization: Bearer` or `x-api-key`.

Exactly one upstream must have `default = true`. That host gets management calls with an unknown name, plus paths that have no model (`/api/ps`, `/api/version`, blobs).

`[[model]]` names are what clients send. `upstream` is the host id. `upstream_model` rewrites JSON `model` and `name` when those keys already exist.

An upstream `api_key` is sent to that Ollama as `Authorization: Bearer`. The client's key is stripped.

`GET /api/tags` and `GET /v1/models` are built from `[[model]]`. They do not probe the hosts.
