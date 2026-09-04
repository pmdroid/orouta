---
title: HTTPS with Tailscale
description: Serve orouta over HTTPS on your tailnet with tailscale serve.
---

Tailscale can give orouta a valid TLS certificate on `https://<host>.<tailnet>.ts.net` with no TLS config of its own. `tailscale serve` proxies to a localhost backend only, so orouta must be bound to `127.0.0.1`, not `0.0.0.0`:

```toml
host = "127.0.0.1"
```

Prereqs: a tailnet device with HTTPS certificates enabled in the Tailscale admin console.

## Serve

```sh
tailscale serve --bg --https=443 http://127.0.0.1:11434
```

orouta is now on `https://<host>.<tailnet>.ts.net`. Point clients at it:

```sh
OLLAMA_HOST=https://host.tail-scale.ts.net ollama run llama3
```

`tailscale serve status` shows what is active. `tailscale serve --https=443 off` removes it.

## Public exposure

Funnel opens the same URL to the internet. It is disabled by default: the first run triggers an approval flow that adds the `funnel` node attribute to the tailnet policy. Like serve, it needs the target:

```sh
tailscale funnel --bg --https=443 http://127.0.0.1:11434
```

Do this only deliberately: `[auth].keys` is strongly recommended once orouta is reachable beyond the tailnet, or anyone can use your upstream hosts.

Tailscale terminates TLS only. orouta still does the routing: host selection, `/v1/messages` translation, and auth.
