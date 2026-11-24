#!/bin/sh
# Extração local AST; ambiente sem chaves, sem backend LLM e sem clustering.
set -eu
graphify_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$graphify_root"
exec env -i HOME="$HOME" PATH=/home/codex/.local/bin:/usr/bin:/bin LANG=C.UTF-8 GRAPHIFY_NO_TIPS=1 /home/codex/.local/bin/graphify update "$graphify_root" --force --no-cluster
