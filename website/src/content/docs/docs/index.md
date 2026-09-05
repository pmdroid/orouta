---
title: orouta
description: Several Ollama hosts behind one port.
---

orouta is a small HTTP proxy. You list Ollama hosts in a TOML file. There is no model list in config. orouta asks each host `/api/tags`. A client talks to orouta as if it were Ollama. The JSON `model` or `name` field picks the host that listed it.

New weights: `POST /api/pull?host=…` routes the download to a host you choose, or run `ollama pull` on the Ollama machine itself. orouta asks each host `/api/tags` and routes by the names it reports.

Ollama's own `/api/*` and `/v1/*` paths are forwarded as-is. `POST /v1/messages` is the one exception: that Anthropic shape is translated to Ollama `/api/chat`.

- [Install](/docs/install/)
- [Config](/docs/config/)
- [Manage](/docs/manage/)
- [Expose Ollama](/docs/ollama-host/)
- [API](/docs/api/)
- [Roadmap](/docs/roadmap/)
