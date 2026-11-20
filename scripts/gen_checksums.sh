#!/usr/bin/env bash
# Writes checksum-Elixir.TantivyEx.Native.exs from the precompiled NIFs of the
# GitHub release of the current version (published by .github/workflows/release.yml).
set -euo pipefail
cd "$(dirname "$0")/.."
mix rustler_precompiled.download TantivyEx.Native --all --print
