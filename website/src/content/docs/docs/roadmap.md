---
title: Roadmap
description: What you can do today, and what might show up next.
---

## Now

- Point one URL at several Ollama machines. Chat goes to the host that already has that model.
- Use Ollama's API, OpenAI `/v1`, or Anthropic `/v1/messages` (text).
- Lock the proxy with API keys, or leave it open on a trusted network.
- List models as the union of every host's `ollama list`.
- Install with `curl -fsSL https://orouta.dev/install.sh | sh` (needs a GitHub release for the binary).

## Next

- [ ] A small web page: which host, which model, errors, how busy
- [x] `ollama pull` through orouta onto a machine you choose
- [ ] See when a host is down instead of a silent miss
- [ ] Change the TOML without restarting
- [ ] HTTPS, or a short guide to put Caddy/nginx in front
- [ ] Homebrew

## Later

- [ ] Same model name on two hosts without guessing
- [ ] Anthropic tools and images, if anyone uses that path
- [ ] A log of who called which model
