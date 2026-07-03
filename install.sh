#!/usr/bin/env bash
# Nolgia CLI installer
#
#   curl -fsSL https://raw.githubusercontent.com/nolgiacorp/nolgia-cli/main/install.sh | bash
#
# Options:
#   --prefix <dir>   install directory (default: /usr/local/bin if writable, else ~/.local/bin)
#   --tag <vX.Y.Z>   release tag to install (default: latest)

set -euo pipefail

REPO="nolgiacorp/nolgia-cli"
PREFIX=""
TAG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      PREFIX="$2"
      shift 2
      ;;
    --tag)
      TAG="$2"
      shift 2
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 1
      ;;
  esac
done

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Darwin)
    # The darwin asset is a universal binary covering x86_64 and arm64.
    asset="nolgia-x86_64-apple-darwin"
    ;;
  Linux)
    case "$arch" in
      x86_64 | amd64)
        asset="nolgia-x86_64-unknown-linux-gnu"
        ;;
      *)
        echo "no prebuilt binary for Linux/$arch yet; install with: cargo install nolgia-cli" >&2
        exit 1
        ;;
    esac
    ;;
  MINGW* | MSYS* | CYGWIN*)
    echo "on Windows, download nolgia-x86_64-pc-windows-msvc.exe from https://github.com/$REPO/releases or install with: cargo install nolgia-cli" >&2
    exit 1
    ;;
  *)
    echo "unsupported platform: $os/$arch; install with: cargo install nolgia-cli" >&2
    exit 1
    ;;
esac

if [ -z "$TAG" ]; then
  TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
    awk -F'"' '/"tag_name"/ {print $4; exit}')
  if [ -z "$TAG" ]; then
    echo "could not resolve the latest release tag; pass one with --tag vX.Y.Z" >&2
    exit 1
  fi
fi

if [ -z "$PREFIX" ]; then
  if [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    PREFIX="/usr/local/bin"
  else
    PREFIX="$HOME/.local/bin"
  fi
fi
mkdir -p "$PREFIX"

url="https://github.com/$REPO/releases/download/$TAG/$asset"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "downloading nolgia $TAG ($asset)..."
curl -fL --progress-bar "$url" -o "$tmp/nolgia"
chmod +x "$tmp/nolgia"

if [ "$os" = "Darwin" ]; then
  xattr -d com.apple.quarantine "$tmp/nolgia" 2>/dev/null || true
fi

mv "$tmp/nolgia" "$PREFIX/nolgia"

config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/nolgia"
mkdir -p "$config_dir"
cat > "$config_dir/install-metadata.json" <<METADATA
{"method":"install.sh","tag":"$TAG","prefix":"$PREFIX","installed_at":"$(date -u +%Y-%m-%dT%H:%M:%SZ)"}
METADATA

echo "installed $("$PREFIX/nolgia" --version) to $PREFIX/nolgia"

case ":$PATH:" in
  *":$PREFIX:"*) ;;
  *)
    echo "note: $PREFIX is not on your PATH; add it with:"
    echo "  export PATH=\"$PREFIX:\$PATH\""
    ;;
esac
