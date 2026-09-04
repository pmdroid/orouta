---
title: Install
description: Install orouta from GitHub releases.
---

```sh
curl -fsSL https://orouta.dev/install.sh | sh
```

The script detects OS and CPU, then downloads the matching binary from the latest GitHub release into `~/.local/bin`. Add that directory to `PATH` if it is not already there.

Pin a release:

```sh
OROUTA_VERSION=v0.1.0 curl -fsSL https://orouta.dev/install.sh | sh
```

Install somewhere else:

```sh
OROUTA_BINDIR=/usr/local/bin curl -fsSL https://orouta.dev/install.sh | sh
```

Linux amd64/arm64 and macOS Intel/Apple Silicon are built on every `v*` tag. Main-branch pushes also upload the same binaries as GitHub Actions artifacts.

Ollama on another machine must bind `0.0.0.0` so orouta can reach it:

```sh
export OLLAMA_HOST=0.0.0.0:11434
```

Restart Ollama after setting that. See [Config](/docs/config/).

From source:

```sh
git clone https://github.com/pmdroid/orouta
cd orouta
cargo run --release -- --config orouta.toml
```
