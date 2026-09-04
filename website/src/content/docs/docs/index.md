---
title: orouta
description: Several Ollama hosts behind one port.
---

orouta is a small HTTP proxy. You list Ollama hosts in a TOML file. There is no model list in config. orouta asks each host `/api/tags`. A client talks to orouta as if it were Ollama. The JSON `model` or `name` field picks the host that listed it.

New weights: `ollama pull` on the Ollama machine itself. orouta does not pick a download host.

Ollama's own `/api/*` and `/v1/*` paths are forwarded as-is. `POST /v1/messages` is the one exception: that Anthropic shape is translated to Ollama `/api/chat`.

- [Install](/docs/install/)
- [Config](/docs/config/)
- [Expose Ollama](/docs/ollama-host/)
- [API](/docs/api/)
- [Roadmap](/docs/roadmap/)
