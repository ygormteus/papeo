#!/bin/sh
# Extração local AST; ambiente sem chaves, sem backend LLM e sem clustering.
set -eu
graphify_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$graphify_root"
# Instalação padrão (uv tool/pipx) em ~/.local/bin; senão, o do PATH.
graphify_bin="$HOME/.local/bin/graphify"
[ -x "$graphify_bin" ] || graphify_bin=$(command -v graphify) || {
    printf '%s\n' 'graphify não encontrado; veja docs/repowise.md' >&2
    exit 127
}
exec env -i HOME="$HOME" PATH="$(dirname -- "$graphify_bin"):/usr/bin:/bin" LANG=C.UTF-8 GRAPHIFY_NO_TIPS=1 "$graphify_bin" update "$graphify_root" --force --no-cluster
