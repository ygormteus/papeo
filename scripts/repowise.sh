#!/bin/sh
set -eu
repowise_project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
repowise_python="${HOME}/.local/share/repowise-local/venvs/0.60.1/bin/python"
if [ ! -x "$repowise_python" ]; then
    printf '%s\n' 'RepoWise runtime missing; see docs/repowise.md' >&2
    exit 127
fi
if [ "${1:-}" = update ] || [ "${1:-}" = init ]; then
    mkdir -p "$repowise_project_root/.repowise"
    exec flock -x "$repowise_project_root/.repowise/update.lock" \
        env -i PATH=/usr/bin:/bin LANG=C.UTF-8 "$repowise_python" "$repowise_project_root/scripts/repowise-entry.py" "$@"
fi
exec env -i PATH=/usr/bin:/bin LANG=C.UTF-8 "$repowise_python" "$repowise_project_root/scripts/repowise-entry.py" "$@"
