#!/bin/sh
# Local post-commit index refresh. Index failures never change the commit result.
set -u

repowise_root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$repowise_root" || exit 0
[ -f .repowise/config.yaml ] && [ -f .repowise/wiki.db ] || exit 0
command -v flock >/dev/null 2>&1 || exit 0
repowise_queue="$repowise_root/.repowise/automation"
mkdir -p "$repowise_queue" || exit 0

repowise_worker() {
    flock -n 9 || return 0
    while :; do
        if [ -e "$repowise_queue/pending" ]; then
            rm -f "$repowise_queue/pending" || return 0
            if [ -f "$repowise_queue/update.log" ]; then
                mv -f "$repowise_queue/update.log" "$repowise_queue/update.previous.log" || return 0
            fi
            if sh "$repowise_root/scripts/repowise.sh" update --progress json >"$repowise_queue/update.log" 2>&1; then
                repowise_result=0
            else
                repowise_result=$?
            fi
            printf 'exit_code=%s\n' "$repowise_result" >"$repowise_queue/status"
            continue
        fi
        # Release before checking again: a request arriving during shutdown can
        # acquire the lock itself, or this worker can drain it after reacquiring.
        flock -u 9 || return 0
        [ -e "$repowise_queue/pending" ] || return 0
        flock -n 9 || return 0
    done
}

if [ "${1:-}" = --worker ]; then
    repowise_worker 9>"$repowise_queue/worker.lock"
    exit 0
fi

: >"$repowise_queue/pending" || exit 0
nohup /bin/sh "$repowise_root/scripts/repowise-post-commit.sh" --worker \
    </dev/null >>"$repowise_queue/worker.log" 2>&1 &
printf '%s\n' 'RepoWise: atualização agendada; log em .repowise/automation/update.log'
exit 0
