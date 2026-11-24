"""Focused privacy and subprocess checks; does not read real app credentials."""
import importlib.util
import os
import socket
import sqlite3
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

entry = Path(__file__).with_name("repowise-entry.py")
spec = importlib.util.spec_from_file_location("project_repowise", entry)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)

runtime.INDEX.joinpath("runtime").mkdir(parents=True, exist_ok=True)
external = Path(tempfile.mkdtemp(prefix="project-repowise-external-"))
canary = external / "ordinary.json"
canary.write_text('{"synthetic": "must-never-be-read"}')
with tempfile.TemporaryDirectory(prefix="check-", dir=runtime.INDEX / "runtime") as temporary:
    bootstrap_key = Path(temporary) / "bootstrap/key"
    with ThreadPoolExecutor(max_workers=8) as pool:
        keys = list(pool.map(lambda _: runtime.cache_signing_key(bootstrap_key), range(32)))
    assert len(set(keys)) == 1 and len(keys[0]) == 32
    assert bootstrap_key.stat().st_mode & 0o777 == 0o600
    assert not list(bootstrap_key.parent.glob(".cache_hmac_key.*"))
    target = Path(temporary) / ".env.private"
    target.write_text("SYNTHETIC_SECRET=must-never-be-read\n")
    alias = Path(temporary) / "alias.ts"
    alias.symlink_to(target)
    ordinary_alias = Path(temporary) / "ordinary.ts"
    ordinary_alias.symlink_to(canary)
    key_alias = Path(temporary) / "cache-key.ts"
    key_alias.symlink_to(runtime.CACHE_KEY)
    sqlite_alias = Path(temporary) / "sqlite-alias.db"
    sqlite_alias.symlink_to(external / "synthetic.db")
    credential_canaries = []
    for i, name in enumerate(("firebase-service-account.json", "firebase-service-account-stage.json", "google-service-account.json", "google-service-account-stage.json", "project-firebase-adminsdk-fixture.json", "GOOGLE-SERVICE-ACCOUNT-UPPER.JSON")):
        credential = Path(temporary) / name
        credential.write_text('{"synthetic": "must-never-be-read"}')
        credential_alias = Path(temporary) / f"credential-{i}.ts"
        credential_alias.symlink_to(credential)
        credential_canaries.extend((credential, credential_alias))
        assert runtime.protected_path(runtime.ROOT / name)
        assert runtime.protected_path(credential)
        assert runtime.protected_path(credential_alias)
    assert runtime.protected_path(canary)
    assert runtime.protected_path(ordinary_alias)
    assert runtime.protected_path(Path.home() / ".claude.json")
    assert runtime.protected_path(runtime.CACHE_KEY)
    assert runtime.protected_path(Path("/opt/project/private-storage/canary.json"))
    assert runtime.protected_path(target)
    assert runtime.protected_path(alias)
    assert runtime.protected_path(runtime.ROOT / "work/private.txt")
    assert runtime.protected_path(runtime.ROOT / "public/uploads/document.txt")
    assert runtime.protected_path(Path.home() / ".codex/auth.json")
    assert not runtime.protected_path(runtime.ROOT / "lib/turma-vagas.ts")
    assert not runtime.protected_path(runtime.INDEX / "wiki.db")
    assert runtime.git_read_only(["git", "-c", "core.quotepath=false", "log", "--oneline"])
    assert not runtime.git_read_only(["git", "fetch"])
    assert not runtime.git_read_only(["git", "reset", "--hard"])
    for unsafe in (["git", "diff", "--output=/tmp/unwanted"], ["git", "show", "--textconv"], ["git", "config", "--get", "--unset", "key"], ["git", "-c", "core.fsmonitor=evil", "status"]):
        assert not runtime.git_read_only(unsafe)
    assert "--no-ext-diff" in runtime.safe_git_argv(["git", "log"])
    assert "--no-textconv" in runtime.safe_git_argv(["git", "diff", "--name-only"])
    os.environ["PROJECT_REPOWISE_TEST_SECRET"] = "synthetic"
    private_sources = [runtime.ROOT / value for value in (
        "apps/web/public/uploads/canary.txt", "plataforma/storage/canary.txt",
        "packages/database/backups/canary.txt", "src/credentials/canary.ts",
        "server/.env.test", "db/canary.sql", "config.local.json",
        "patient-export.json", "client/layout.json", "client/model.json",
        "nested/GOOGLE-SERVICE-ACCOUNT.json", "nested/Uploads/canary.txt",
    )]
    for private in private_sources:
        assert runtime.protected_path(private), str(private)
    assert not runtime.protected_path(runtime.ROOT / "src/example.ts")
    assert not runtime.protected_path(runtime.ROOT / "package.json")
    for argv in (["git", "show"], ["git", "diff"], ["git", "diff", "-U0", "HEAD", "--"], ["git", "show", "HEAD:.env"], ["git", "cat-file", "-p", "HEAD:Uploads/private.ts"], ["git", "-C", "/tmp", "status"]):
        assert not runtime.git_read_only(argv)
    assert not runtime.git_read_only(["git", "log", "--patch"])
    for metadata_mode in ("--stat", "--raw", "--numstat", "--name-only", "--name-status", "--shortstat", "--summary"):
        assert not runtime.git_read_only(["git", "diff", "HEAD~1", "HEAD", "--", ".env", metadata_mode])
    for option in ("--cont=.env", "--ignore-revs-fil=.env", "--textcon", "-S.env"):
        assert not runtime.git_read_only(["git", "blame", option, "--", "src/example.ts"])
    runtime.configure_runtime()
    from repowise.core.ingestion.traverser import FileTraverser
    traverser = FileTraverser(runtime.ROOT)
    for private in private_sources:
        assert traverser._build_file_info(private) is None
    assert traverser._should_skip_dir("Uploads", Path("nested/Uploads"))
    for database in (external / "synthetic.db", sqlite_alias):
        try:
            sqlite3.connect(database)
        except PermissionError:
            pass
        else:
            raise AssertionError("SQLite opened outside this index")
    sqlite3.connect(":memory:").close()
    for argv in (["git", "diff", "-U0", "HEAD", "--"], ["git", "show", "HEAD:.env"]):
        try:
            subprocess.run(argv, check=True)
        except PermissionError:
            pass
        else:
            raise AssertionError("Git content guard was bypassed")
    try:
        subprocess.run(["git", "cat-file", "--batch"], input=b"HEAD:.env\n", capture_output=True, check=False)
    except PermissionError:
        pass
    else:
        raise AssertionError("Git batch read a protected object")
    from repowise.core.cache_seal import seal, unseal
    sealed = runtime.INDEX / "runtime/check-persistent-seal.bin"
    if "--write-seal" in sys.argv:
        sealed.write_bytes(seal(b"synthetic-cache", domain="project-runtime-test"))
    elif "--read-seal" in sys.argv:
        assert unseal(sealed.read_bytes(), domain="project-runtime-test") == b"synthetic-cache"
        sealed.unlink()
    assert "PROJECT_REPOWISE_TEST_SECRET" not in os.environ
    from repowise.cli import ui
    assert ui.load_dotenv(runtime.ROOT) is None
    from pydantic_settings import BaseSettings, SettingsConfigDict
    class SyntheticSettings(BaseSettings):
        model_config = SettingsConfigDict(env_file=target)
    assert SyntheticSettings().model_dump() == {}
    for private in (target, alias, canary, ordinary_alias, runtime.CACHE_KEY, key_alias, *credential_canaries, *private_sources):
        try:
            private.read_text()
        except PermissionError:
            pass
        else:
            raise AssertionError("private file was readable")
    try:
        socket.getaddrinfo("example.com", 443)
    except OSError:
        pass
    else:
        raise AssertionError("network resolution was allowed")
    for argv in (["git", "fetch"], ["sh", "-c", "true"]):
        try:
            subprocess.run(argv, check=True)
        except PermissionError:
            pass
        else:
            raise AssertionError("unexpected subprocess was allowed")
    assert subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip() == str(runtime.ROOT)
canary.unlink()
external.rmdir()
print("RepoWise runtime: privacy, symlink, keyless env and read-only subprocess checks passed")
