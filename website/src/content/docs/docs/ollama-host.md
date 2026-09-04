---
title: Expose Ollama on the LAN
description: Bind Ollama to 0.0.0.0 so orouta can reach it.
---

Ollama binds `127.0.0.1:11434` by default. A LAN IP then returns connection refused. Set `OLLAMA_HOST=0.0.0.0:11434` on that machine. Steps below are from the [Ollama FAQ](https://docs.ollama.com/faq#how-can-i-expose-ollama-on-my-network).

Leave Ollama on loopback if the upstream is `http://127.0.0.1:11434` on the same box as orouta.

## macOS

If you run the Ollama app, set the variable with `launchctl`, then restart the app:

```sh
launchctl setenv OLLAMA_HOST "0.0.0.0:11434"
```

## Linux

If Ollama is a systemd service:

```sh
sudo systemctl edit ollama.service
```

Add:

```ini
[Service]
Environment="OLLAMA_HOST=0.0.0.0:11434"
```

Then:

```sh
sudo systemctl daemon-reload
sudo systemctl restart ollama
```

`export` in your shell does not change the systemd process.

## Windows

1. Quit Ollama from the taskbar.
2. Open Settings (Windows 11) or Control Panel (Windows 10) and search for environment variables.
3. Edit environment variables for your account.
4. Create or edit `OLLAMA_HOST` with value `0.0.0.0:11434`.
5. OK/Apply, then start Ollama from the Start menu.

## Check

On the Ollama machine, `curl http://127.0.0.1:11434/api/tags` should work. From the orouta machine, `curl http://<lan-ip>:11434/api/tags` should work too. Then orouta will list that host's models on the next tags refresh.
