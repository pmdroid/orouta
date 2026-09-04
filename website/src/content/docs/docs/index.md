---
title: orouta
description: Several Ollama hosts behind one port.
---

orouta is a small HTTP proxy. You list Ollama hosts in a TOML file. orouta asks each host which models it has. A client talks to orouta as if it were Ollama. The `model` field in the JSON body picks which host gets the request. A name should exist on only one host.

Ollama's own `/api/*` and `/v1/*` paths are forwarded as-is. `POST /v1/messages` is the one exception: that Anthropic shape is translated to Ollama `/api/chat`.

- [Install](/docs/install/)
- [Config](/docs/config/)
- [API](/docs/api/)
