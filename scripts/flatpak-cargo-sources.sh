#!/usr/bin/env bash
# Refresh packaging/flatpak/cargo-sources.json after Cargo.lock changes.
# Flatpak builds offline, so every crate is listed there as a source.
#
#   ./scripts/flatpak-cargo-sources.sh
#
# Runs flatpak-cargo-generator (pinned below) with python3 when it has aiohttp
# and tomlkit, otherwise with the Python in the Freedesktop SDK from the manifest.

set -euo pipefail

GENERATOR_COMMIT=41c20aa10819cdb2a4f3ca171758a96d1955c018
GENERATOR_SHA256=0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380
GENERATOR_URL="https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$GENERATOR_COMMIT/cargo/flatpak-cargo-generator.py"
SDK=org.freedesktop.Sdk//26.08

repo="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo/packaging/flatpak/cargo-sources.json"
work="$(mktemp -d "${XDG_CACHE_HOME:-$HOME/.cache}/cinebox-cargo-sources.XXXXXX")"
trap 'rm -rf "$work"' EXIT

curl -fsSL --retry 3 -o "$work/generator.py" "$GENERATOR_URL"
echo "$GENERATOR_SHA256  $work/generator.py" | sha256sum --check --strict --quiet

if python3 -c 'import aiohttp, tomlkit' 2>/dev/null; then
  python3 "$work/generator.py" "$repo/Cargo.lock" -o "$out"
elif flatpak info "$SDK" >/dev/null 2>&1; then
  flatpak run --share=network --filesystem="$repo" --filesystem="$work" --command=sh "$SDK" -c '
    set -e
    python3 -m pip install --quiet --target "$1/pydeps" aiohttp tomlkit
    PYTHONPATH="$1/pydeps" python3 "$1/generator.py" "$2/Cargo.lock" -o "$3"
  ' sh "$work" "$repo" "$out"
else
  echo "Need python3 with aiohttp and tomlkit, or flatpak with $SDK installed." >&2
  exit 1
fi

echo "Wrote $out"
