---
title: Roadmap
description: What might land next.
---

Check items off in this file as they ship.

## Ship

- [ ] Tag `v0.1.0` so `curl | sh` has binaries
- [ ] Cloudflare Pages on `orouta.dev` (`website/`, output `dist`)
- [ ] systemd unit
- [ ] launchd plist

## Proxy

- [ ] SIGHUP / file watch to reload TOML
- [ ] Per-host up/down in `/api/tags`
- [ ] Same name on two hosts: 409 or prefer running (`/api/ps`)
- [ ] Pull host later (`pull = true` on one upstream)
- [ ] Refresh tags right after a successful pull

## Stats

- [ ] `GET /stats` JSON (per host + per model: count, errors, latency, last used)
- [ ] One HTML page that polls it
- [ ] In-flight stream count

## Protocol

- [ ] Anthropic tools/images, or drop `/v1/messages` if unused
- [ ] TLS, or document Caddy/nginx in front

## Ops

- [ ] Access log: time, model, host, status, ms
- [ ] Homebrew formula
