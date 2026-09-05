---
title: Config
description: TOML config for hosts and API keys.
---

Copy the example and edit it:

```sh
cp orouta.toml.example orouta.toml
orouta --config orouta.toml
```

`--config` defaults to `orouta.toml` in the working directory. Listen defaults are `host = "0.0.0.0"` and `port = 11434`.

```toml
host = "0.0.0.0"
port = 11434

[auth]
keys = ["sk-orouta-alice"]

[[upstream]]
id = "home"
base_url = "http://192.168.1.10:11434"
api_key = ""

[[upstream]]
id = "desk"
base_url = "http://192.168.1.20:11434"
```

There is no model list in the TOML. orouta calls `GET /api/tags` on each host and routes by those names. Put a given model on one host. Clients can send `llama3` when the host reports `llama3:latest`.

To download a new model, `POST /api/pull?host=<id>` through orouta (see [API](/docs/api/)) or run `ollama pull` on that Ollama machine. orouta will see it on the next tags refresh.

Each remote Ollama must listen on `0.0.0.0`, not only loopback. Otherwise orouta gets connection refused on the LAN IP. How to set that on macOS, Linux, and Windows is in [Expose Ollama](/docs/ollama-host/). Localhost-only Ollama is fine for an upstream of `http://127.0.0.1:11434`.

`auth.keys` empty means no client auth. Otherwise send `Authorization: Bearer` or `x-api-key`.

orouta reloads the TOML about a second after you save it — no restart needed. Listen address changes need a restart. Hosts and keys changed in the UI live in `orouta.overlay.json` next to the TOML and survive reloads; see [Manage](/docs/manage/).

Unknown names are 404. Paths with no model (`/api/ps`, `/api/version`, blobs) go to the first host in the file.

An upstream `api_key` is sent to that Ollama as `Authorization: Bearer`. The client's key is stripped.

`GET /api/tags` and `GET /v1/models` are the union of what the hosts report.
