#!/bin/sh
set -eu

REPO="${OROUTA_GITHUB_REPO:-pmdroid/orouta}"
BINDIR="${OROUTA_BINDIR:-${HOME}/.local/bin}"
VERSION="${OROUTA_VERSION:-latest}"

os=$(uname -s | tr '[:upper:]' '[:lower:]')
arch=$(uname -m)

case "${os}-${arch}" in
  linux-x86_64|linux-amd64) target=x86_64-unknown-linux-gnu ;;
  linux-aarch64|linux-arm64) target=aarch64-unknown-linux-gnu ;;
  darwin-arm64) target=aarch64-apple-darwin ;;
  darwin-x86_64) target=x86_64-apple-darwin ;;
  *)
    echo "unsupported: ${os} ${arch}" >&2
    exit 1
    ;;
esac

if [ "${1:-}" = "--print-target" ]; then
  echo "$target"
  exit 0
fi

asset="orouta-${target}"
if [ "$VERSION" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${asset}"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
fi

if [ "${1:-}" = "--print-url" ]; then
  echo "$url"
  exit 0
fi

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp"
chmod +x "$tmp"
mkdir -p "$BINDIR"
mv "$tmp" "${BINDIR}/orouta"
trap - EXIT

echo "installed ${BINDIR}/orouta"
case ":${PATH}:" in
  *":${BINDIR}:"*) ;;
  *) echo "add ${BINDIR} to PATH" ;;
esac
