"""Exercise Git's hook dispatch in an isolated repo with a synthetic indexer."""
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

SOURCE = Path(__file__).with_name("repowise-post-commit.sh")


def wait_for(predicate):
    deadline = time.monotonic() + 10
    while not predicate():
        if time.monotonic() >= deadline:
            raise AssertionError("post-commit worker did not reach the expected state")
        time.sleep(0.03)


with tempfile.TemporaryDirectory(prefix="repowise-hook-") as temporary:
    root = Path(temporary) / "repo with spaces"
    root.mkdir()
    subprocess.run(["git", "init", "-q", str(root)], check=True)
    subprocess.run(["git", "-C", str(root), "config", "core.hooksPath", str(root / ".git/hooks")], check=True)
    scripts = root / "scripts"
    scripts.mkdir()
    driver = scripts / SOURCE.name
    shutil.copyfile(SOURCE, driver)
    driver.chmod(0o700)
    (root / ".git/hooks/post-commit").symlink_to(driver)
    state = root / ".repowise"
    state.mkdir()
    (state / "config.yaml").write_text("synthetic: true\n")
    (state / "wiki.db").touch()
    (state / "block").touch()
    (scripts / "repowise.sh").write_text('''#!/bin/sh
set -eu
if ! mkdir .repowise/active; then
    touch .repowise/overlap
    exit 99
fi
trap 'rmdir .repowise/active' EXIT
printf 'run\n' >> .repowise/runs
while [ -e .repowise/block ] && [ ! -e .repowise/release ]; do sleep 0.03; done
if [ -e .repowise/fail ]; then exit 17; fi
''')

    def hook():
        subprocess.run(["git", "-C", str(root), "hook", "run", "post-commit"],
                       check=True, capture_output=True, timeout=5)

    def runs():
        path = state / "runs"
        return len(path.read_text().splitlines()) if path.exists() else 0

    def idle():
        lock = state / "automation/worker.lock"
        return lock.exists() and not (state / "automation/pending").exists() and subprocess.run(
            ["flock", "-n", str(lock), "true"], check=False, capture_output=True,
        ).returncode == 0

    try:
        hook()  # Returns while the synthetic indexer is deliberately blocked.
        wait_for(lambda: runs() == 1)
        for _ in range(12):
            hook()
        assert runs() == 1
        (state / "release").touch()
        wait_for(lambda: runs() == 2 and idle())
        assert not (state / "overlap").exists()
        assert (state / "automation/status").read_text() == "exit_code=0\n"
        (state / "fail").touch()
        hook()  # A failing indexer still leaves the hook successful.
        wait_for(lambda: runs() == 3 and idle())
        assert (state / "automation/status").read_text() == "exit_code=17\n"
        assert (state / "automation/update.previous.log").exists()
    finally:
        (state / "release").touch()
        wait_for(idle)
print("RepoWise post-commit: async dispatch, coalescing, exclusion and failure isolation passed")
