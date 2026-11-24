"""Local, keyless RepoWise entrypoint for this checkout; never load app env files."""
from __future__ import annotations

import os
import secrets
import socket
import subprocess
import sys
import sysconfig
import threading
import tempfile
from fnmatch import fnmatchcase
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INDEX = ROOT / ".repowise"
RUNTIME = Path.home() / ".local/share/repowise-papeo"
CACHE_KEY = RUNTIME / "cache_hmac_key"
TOOLS = "get_context,search_codebase,get_symbol,get_risk,get_overview,get_answer"
PRIVATE_DIRS = {"work", "var", "worktrees", "output", "outputs", "tmp", ".tmp", "uploads", "storage", "private-storage", "backups", "backup", "data", "medical_backup", "task_summaries", "db", "exports", "imports", "logs", "cache", "coverage", "test-results", "playwright-report", "node_modules", "vendor", "__pycache__", "target", "gen", "graphify-out", "debug", "dxf", "dwg", "renders", ".git", ".codex", ".claude", ".serena", ".agents", ".superpowers", ".venv", ".cache", ".build", ".swc", ".ssh", ".aws", "credentials", "secrets"}
SECRET_SUFFIXES = {".pem", ".key", ".p12", ".pfx", ".sqlite", ".sqlite3", ".db", ".csv", ".xls", ".xlsx", ".pdf", ".doc", ".docx", ".rar", ".zip", ".7z", ".tar", ".gz", ".dump", ".bak", ".log", ".dcm", ".dicom", ".dwg", ".dxf"}
SECRET_NAMES = ("*-firebase-adminsdk-*.json", "firebase-service-account*.json", "google-service-account*.json", "*service-account*.json", "*service_account*.json", "*credentials*.json", "auth.json", "*.local.json", "*.local.yaml", "*.local.yml", "model.json", "layout.json", "estoque-*.json")
PUBLIC_JSON = {"package.json", "components.json", "manifest.json", "composer.json"}
SKIP_BUILD = True
PRIVATE_PATHS = []
EXCLUDES = [".env*", "**/.env*", "**/*.json", "*.sql", "**/*.sql", "**/.*venv*/**", "**/.next*/**", "**/.svelte-kit/**", "**/.turbo/**", "**/build/**", "**/dev-dist/**", *(["**/dist/**"] if SKIP_BUILD else []), *("**/" + folder + "/**" for folder in sorted(PRIVATE_DIRS)), *("**/" + pattern for pattern in SECRET_NAMES), *PRIVATE_PATHS, *("**/*" + suffix for suffix in sorted(SECRET_SUFFIXES))]
HOME_PRIVATE = tuple(Path.home() / name for name in (
    ".codex", ".claude", ".claude.json", ".repowise", ".ssh", ".aws", ".config", ".git-credentials", ".netrc",
))
LIBRARY_ROOTS = (Path(sys.prefix), Path(sys.prefix).resolve(), Path(sysconfig.get_path("stdlib")), Path("/usr/share/zoneinfo"))
SYSTEM_FILES = {Path(p).resolve() for p in ("/dev/null", "/etc/os-release", "/etc/localtime", "/etc/timezone", "/etc/mime.types")}
READ_GIT = {"version", "log", "show", "diff", "status", "ls-files", "rev-parse", "rev-list", "blame", "cat-file", "for-each-ref", "merge-base", "check-ignore", "check-attr"}


def in_index(path: Path) -> bool:
    try:
        first = path.relative_to(ROOT).parts[0]
        return first == ".repowise" or first.startswith(".repowise-seed-")
    except (ValueError, IndexError):
        return False


def protected_path(value: str | bytes | os.PathLike) -> bool:
    path = Path(os.fsdecode(value)).resolve()
    name = path.name.lower()
    if name.startswith(".env") or any(fnmatchcase(name, pattern) for pattern in SECRET_NAMES) or any(path == p or p in path.parents for p in HOME_PRIVATE):
        return True
    if in_index(path):
        return False
    if path in SYSTEM_FILES or any(path == p or p in path.parents for p in LIBRARY_ROOTS):
        return False
    if path != ROOT and ROOT not in path.parents:
        return True
    if path.suffix.lower() in SECRET_SUFFIXES:
        return True
    if path.suffix.lower() == ".sql":
        return True
    git_dir = ROOT / ".git"
    if path in {git_dir / name for name in ("HEAD", "config", "index", "packed-refs", "commondir", "gitdir", "info/exclude")} or git_dir / "refs" in path.parents:
        return False
    try:
        relative = path.relative_to(ROOT)
        parts = tuple(part.lower() for part in relative.parts)
        if any(part in PRIVATE_DIRS or part.startswith((".next", ".venv")) or part in {".svelte-kit", ".turbo", "build", "dev-dist"} or (SKIP_BUILD and part == "dist") for part in parts):
            return True
        if any(fnmatchcase(relative.as_posix().lower(), pattern) for pattern in PRIVATE_PATHS):
            return True
        return path.suffix.lower() == ".json" and name not in PUBLIC_JSON and not name.startswith("tsconfig")
    except ValueError:
        return True


def safe_source_path(value: str) -> bool:
    if not value or value.startswith(("-", ":")) or any(c in value for c in "*?[]\n\r\x00"):
        return False
    relative = Path(value)
    if relative.is_absolute() or ".." in relative.parts:
        return False
    if any(part == ".git" or part.startswith(".repowise") for part in relative.parts):
        return False
    target = ROOT / relative
    resolved = target.resolve()
    return ROOT in resolved.parents and not protected_path(target) and not target.is_dir()


def safe_blob_spec(value: str) -> bool:
    if ":" not in value:
        return False
    revision, path = value.split(":", 1)
    return bool(revision and not revision.startswith("-") and not any(c.isspace() or c == "\x00" for c in revision) and safe_source_path(path))


def metadata_diff_args(args: list[str]) -> bool:
    modes = {"--name-only", "--name-status", "--raw", "--numstat", "--stat", "--shortstat", "--summary"}
    flags = modes | {"--no-ext-diff", "--no-textconv", "--no-renames", "--no-color", "--full-index", "--no-commit-id", "--root", "--cached", "--staged", "--relative", "--exit-code", "--quiet", "-z", "-r", "-M", "-C", "-B"}
    before_paths = args[:args.index("--")] if "--" in args else args
    if not any(arg in modes for arg in before_paths):
        return False
    for arg in before_paths:
        if arg in flags:
            continue
        if arg.startswith("--abbrev=") and arg.split("=", 1)[1].isdigit():
            continue
        if arg.startswith("--diff-filter=") and arg.split("=", 1)[1].isalpha():
            continue
        if arg.startswith("-") or ":" in arg or any(c in arg for c in "\n\r\x00"):
            return False
    return True


def git_read_only(argv: list[str]) -> bool:
    forbidden = {"--ext-diff", "--textconv", "--filters", "--output", "--exec", "--open-files-in-pager", "--contents", "--exclude-from", "--git-dir", "--work-tree", "--batch-command", "--batch-all-objects", "--no-index", "--show-signature"}
    if any(a.split("=", 1)[0] in forbidden or a.startswith("-L") or "\x00" in a for a in argv):
        return False
    args = list(argv[1:])
    while args and args[0].startswith("-"):
        option = args.pop(0)
        if option in {"--version", "--exec-path"}:
            return not args
        if option in {"-c", "-C"} and args:
            value = args.pop(0)
            if option == "-C" and Path(value).resolve() != ROOT:
                return False
            if option == "-c" and value.lower().split("=", 1)[0] not in {"core.quotepath", "color.ui"} and value not in {"core.fsmonitor=false", "core.hooksPath=/dev/null", "core.pager=cat", "log.showSignature=false"}:
                return False
        elif option not in {"--no-pager", "--no-optional-locks"}:
            return False
    if not args:
        return False
    command = args[0]
    if command == "branch":
        return args == ["branch", "--show-current"]
    if command == "config":
        keys = {"core.bare", "core.filemode", "core.ignorecase", "core.logallrefupdates", "core.precomposeunicode", "core.repositoryformatversion", "core.worktree", "extensions.objectformat"}
        return len(args) == 3 and args[1] in {"--get", "--get-all"} and args[2].lower() in keys
    if command == "cat-file":
        if args[1:] in (["--batch"], ["--batch-check"]):
            return True
        if len(args) == 3 and args[1] in {"-t", "-s"}:
            return bool(args[2] and not args[2].startswith("-") and not any(c.isspace() for c in args[2]))
        return len(args) == 3 and args[1] == "-p" and (safe_blob_spec(args[2]) or authorized_blob(args[2]))
    if command == "show":
        specs = [a for a in args[1:] if a not in {"--no-ext-diff", "--no-textconv"}]
        return bool(specs) and all(safe_blob_spec(a) for a in specs)
    if command in {"diff", "diff-tree"}:
        return metadata_diff_args(args[1:])
    if command == "blame":
        if "--" not in args:
            return False
        prefix = args[1:args.index("--")]
        flags = {"--line-porcelain", "--porcelain", "--incremental", "--show-name", "--show-number", "--show-email", "--long", "--root", "--no-textconv", "--no-ignore-revs-file", "-l", "-p", "-w", "-e", "-n", "-f", "-t"}
        index = 0
        revisions = 0
        while index < len(prefix):
            option = prefix[index]
            if option in flags:
                pass
            elif option == "--ignore-revs-file" or option.startswith("--ignore-revs-file="):
                if option == "--ignore-revs-file":
                    index += 1
                    if index >= len(prefix):
                        return False
                    path = prefix[index]
                else:
                    path = option.split("=", 1)[1]
                if not safe_source_path(os.path.relpath(path, ROOT) if Path(path).is_absolute() else path):
                    return False
            elif option.startswith("-") or ":" in option or any(c.isspace() for c in option):
                return False
            else:
                revisions += 1
                if revisions > 1:
                    return False
            index += 1
        paths = args[args.index("--") + 1:]
        return len(paths) == 1 and safe_source_path(paths[0])
    if command == "log":
        for arg in args[1:]:
            if arg.startswith(("--patch", "--word-diff", "--diff-merges", "--combined", "--cc", "--binary")) or arg == "-c":
                return False
            if arg.startswith("-") and not arg.startswith("--") and any(c in arg[1:] for c in "puc"):
                return False
        return True
    return command in READ_GIT - {"show", "diff", "blame", "cat-file", "config", "log"}


_blob_lock = threading.RLock()
_blob_permissions: dict[tuple[int, str], int] = {}


def authorized_blob(value: str) -> bool:
    with _blob_lock:
        return bool(_blob_permissions.get((threading.get_ident(), value), 0))


def safe_git_argv(argv: list[str]) -> list[str]:
    if not git_read_only(argv):
        raise PermissionError("RepoWise local: unsafe Git arguments are disabled")
    result = ["/usr/bin/git", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "core.pager=cat", "-c", "log.showSignature=false", *argv[1:]]
    for i, arg in enumerate(result[1:], 1):
        if arg in {"diff", "diff-tree", "show", "log"}:
            result[i + 1:i + 1] = ["--no-ext-diff", "--no-textconv"]
            if arg == "log":
                result.insert(result.index("--") if "--" in result else len(result), "--no-patch")
            break
    if "blame" in result:
        result[result.index("blame") + 1:result.index("blame") + 1] = ["--no-ignore-revs-file", "--no-textconv"]
    return result


def audit_guard(event: str, args: tuple) -> None:
    if event.startswith("socket.") and event in {
        "socket.connect", "socket.getaddrinfo", "socket.gethostbyname", "socket.gethostbyaddr", "socket.sendto", "socket.bind",
    }:
        if event in {"socket.connect", "socket.bind", "socket.sendto"} and args[0].family == socket.AF_UNIX:
            return
        raise OSError("RepoWise local: network access is disabled")
    if event == "sqlite3.connect" and str(args[0]) != ":memory:" and (str(args[0]).startswith("file:") or not in_index(Path(args[0]).resolve())):
        raise PermissionError("RepoWise local: SQLite is limited to this index")
    if event == "open" and isinstance(args[0], (str, bytes, os.PathLike)):
        if protected_path(args[0]):
            raise PermissionError("RepoWise local: private file access is disabled")
        path = Path(os.fsdecode(args[0])).resolve()
        mode, flags = args[1], args[2]
        writing = (isinstance(mode, str) and any(c in mode for c in "wax+")) or bool(flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC))
        if writing and (path == ROOT or ROOT in path.parents) and not in_index(path):
            raise PermissionError("RepoWise local: writes are limited to .repowise")
    if event == "subprocess.Popen":
        executable = os.fsdecode(args[0])
        argv = [os.fsdecode(x) for x in args[1]]
        if Path(executable).name == "git" and git_read_only(argv):
            return
        if Path(executable).resolve() == Path(sys.executable).resolve() and len(argv) > 2 and argv[1] == "-c" and argv[2].startswith("from multiprocessing."):
            return
        raise PermissionError("RepoWise local: only read-only git and parser subprocesses are allowed")


def cache_signing_key(key_path: Path = CACHE_KEY) -> bytes:
    key_path.parent.mkdir(parents=True, exist_ok=True)
    if not key_path.exists():
        fd, temporary = tempfile.mkstemp(prefix=".cache_hmac_key.", dir=key_path.parent)
        try:
            with os.fdopen(fd, "wb") as stream:
                stream.write(secrets.token_bytes(32))
                stream.flush()
                os.fsync(stream.fileno())
            try:
                # Publish complete bytes without replacing another process's key.
                os.link(temporary, key_path)
            except FileExistsError:
                pass
        finally:
            Path(temporary).unlink(missing_ok=True)
    if key_path.is_symlink():
        raise SystemExit("RepoWise local: cache signing key cannot be a symlink")
    key = key_path.read_bytes()
    if len(key) != 32:
        raise SystemExit("RepoWise local: invalid runtime cache signing key")
    return key


def configure_runtime() -> None:
    os.chdir(ROOT)
    # Bootstrap only our tool's key before installing the source-read guard.
    signing_key = cache_signing_key()
    temporary = INDEX / "runtime/tmp"
    temporary.mkdir(parents=True, exist_ok=True)
    tempfile.tempdir = str(temporary)
    os.environ.clear()
    os.environ.update({
        "PATH": str(Path(sys.executable).parent) + ":/usr/bin:/bin",
        "LANG": "C.UTF-8", "DO_NOT_TRACK": "1", "REPOWISE_TELEMETRY_DISABLED": "1",
        "REPOWISE_SKIP_EDITOR_SETUP": "1", "REPOWISE_NO_SAVE_KEY": "1",
        "REPOWISE_NO_COST_TRACKING": "1", "REPOWISE_EMBEDDER": "mock",
        "HF_HUB_OFFLINE": "1", "TRANSFORMERS_OFFLINE": "1",
        "LITELLM_LOCAL_MODEL_COST_MAP": "True", "PYTHONDONTWRITEBYTECODE": "1",
        "GIT_NO_LAZY_FETCH": "1", "GIT_NO_REPLACE_OBJECTS": "1",
        "GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1",
        "TMPDIR": str(temporary), "GIT_PAGER": "cat", "GIT_OPTIONAL_LOCKS": "0",
        "REPOWISE_CACHE_HMAC_KEY": signing_key.hex(),
    })
    sys.dont_write_bytecode = True
    original_popen = subprocess.Popen

    def object_type(spec):
        with original_popen(["/usr/bin/git", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "cat-file", "-t", spec], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as probe:
            kind = probe.communicate(timeout=10)[0].strip()
            return kind if probe.returncode == 0 else None

    def validate_spec(spec):
        if safe_blob_spec(spec):
            if object_type(spec) != b"blob":
                raise PermissionError("RepoWise local: source selector must identify a file")
            return
        if authorized_blob(spec):
            return
        if not spec or spec.startswith("-") or ":" in spec or any(c.isspace() for c in spec):
            raise PermissionError("RepoWise local: invalid Git object request")
        if object_type(spec) not in {b"commit", b"tree", b"tag"}:
            raise PermissionError("RepoWise local: opaque Git blob access is disabled")

    def validate_batch(data):
        if not data:
            return
        if isinstance(data, bytes):
            data = data.decode("utf-8")
        for spec in data.splitlines():
            validate_spec(spec)

    class CheckedGitInput:
        def __init__(self, stream):
            self.stream = stream

        def write(self, data):
            validate_batch(data)
            return self.stream.write(data)

        def writelines(self, lines):
            lines = list(lines)
            for line in lines:
                validate_batch(line)
            return self.stream.writelines(lines)

        def __getattr__(self, name):
            return getattr(self.stream, name)

    class LocalPopen(original_popen):
        def __init__(self, args, *positional, **kwargs):
            self.checked_batch = False
            if isinstance(args, (list, tuple)) and args and Path(os.fsdecode(args[0])).name == "git":
                if Path(kwargs.get("cwd") or ROOT).resolve() != ROOT:
                    raise PermissionError("RepoWise local: Git is limited to this checkout")
                args = safe_git_argv([os.fsdecode(a) for a in args])
                if "show" in args:
                    for spec in args[args.index("show") + 1:]:
                        if spec not in {"--no-ext-diff", "--no-textconv"}:
                            validate_spec(spec)
                if "cat-file" in args and "-p" in args:
                    validate_spec(args[-1])
                self.checked_batch = "cat-file" in args and "--batch" in args
                if self.checked_batch and kwargs.get("stdin") != subprocess.PIPE:
                    raise PermissionError("RepoWise local: Git batch requires validated stdin")
                kwargs.pop("executable", None)
            super().__init__(args, *positional, **kwargs)
            if self.checked_batch:
                self.stdin = CheckedGitInput(self.stdin)

        def communicate(self, input=None, timeout=None):
            if self.checked_batch:
                validate_batch(input)
            return super().communicate(input=input, timeout=timeout)

    subprocess.Popen = LocalPopen
    sys.addaudithook(audit_guard)
    from repowise.core.blas_threads import limit_blas_threads
    limit_blas_threads()
    from repowise.cli import helpers, ui
    from repowise.cli.ui import env_persistence
    def global_dir() -> Path:
        target = INDEX / "runtime"
        target.mkdir(parents=True, exist_ok=True)
        return target
    helpers.user_global_dir = global_dir
    def no_dotenv(*_args, **_kwargs):
        return None
    ui.load_dotenv = no_dotenv
    env_persistence.load_dotenv = ui.load_dotenv
    # FastMCP's settings use their own dotenv source, separately from the CLI.
    from pydantic_settings.sources import DotEnvSettingsSource
    def no_env_files(_self):
        return {}
    DotEnvSettingsSource._read_env_files = no_env_files

    # Filter before metadata, structure or historical blob extraction sees paths.
    from repowise.core.ingestion.traverser import FileTraverser
    original_walk = FileTraverser._walk
    original_info = FileTraverser._build_file_info
    original_skip = FileTraverser._should_skip_dir

    def public_skip(self, dirname, rel_path, dir_ignore=None):
        return protected_path(self.repo_root / rel_path) or original_skip(self, dirname, rel_path, dir_ignore)

    FileTraverser._should_skip_dir = public_skip

    def public_walk(self):
        for path in original_walk(self):
            if not protected_path(path):
                yield path

    def public_info(self, abs_path):
        return None if protected_path(abs_path) else original_info(self, abs_path)

    FileTraverser._walk = public_walk
    FileTraverser._build_file_info = public_info
    from repowise.core.ingestion.change_detector import ChangeDetector
    original_diff = ChangeDetector._file_diff_from_item

    def public_diff(self, item, *args, **kwargs):
        paths = (getattr(item, "a_path", None), getattr(item, "b_path", None))
        if any(path and protected_path(self.repo_path / path) for path in paths):
            return None
        return original_diff(self, item, *args, **kwargs)

    def no_historical_blob(self, _blob, _path):
        # Current sources remain available; opaque historical blobs are excluded.
        return None

    original_added = ChangeDetector._added_from_disk

    def public_added(self, path, abs_path, *args, **kwargs):
        return None if protected_path(abs_path) else original_added(self, path, abs_path, *args, **kwargs)

    def filter_diffs(method):
        def public_diffs(self, *args, **kwargs):
            return [item for item in method(self, *args, **kwargs) if item is not None and not protected_path(self.repo_path / item.path)]
        return public_diffs

    for method in ("get_changed_files", "get_working_tree_changes", "stale_working_tree_diffs"):
        setattr(ChangeDetector, method, filter_diffs(getattr(ChangeDetector, method)))
    ChangeDetector._added_from_disk = public_added
    ChangeDetector._file_diff_from_item = public_diff
    ChangeDetector._parse_blob = no_historical_blob
    from repowise.core.ingestion.git_indexer import GitIndexer

    async def no_git_history(self, *args, **kwargs):
        return None, []

    GitIndexer.index_repo = no_git_history


def main() -> None:
    allowed = {"init", "update", "mcp", "search", "context", "symbol", "status", "risk", "ask", "--version", "--help"}
    command = sys.argv[1] if len(sys.argv) > 1 else "--help"
    forbidden = {"--provider", "--model", "--prose", "--docs", "--full", "--all", "--editor-setup", "--save-key", "--hook", "--distill-hook", "--codex", "--agents", "--tools", "--embedder"}
    if command not in allowed or any(arg.split("=", 1)[0] in forbidden for arg in sys.argv[2:]):
        raise SystemExit("RepoWise local: use the keyless CLI commands documented in docs/repowise.md")
    if command in {"init", "mcp", "update"}:
        sys.argv.insert(2, str(ROOT))
    if command == "mcp":
        sys.argv.extend(["--no-workspace", "--transport", "stdio", "--tools", TOOLS])
    elif command == "init":
        sys.argv.extend(["--yes", "--no-prose", "--embedder", "mock", "--no-workspace", "--no-editor-setup", "--no-hook", "--no-distill-hook", "--no-agents", "--no-codex", "--no-claude-md", "--no-save-key", "--no-cost-tracking"])
        for pattern in EXCLUDES:
            sys.argv.extend(["--exclude", pattern])
    elif command == "update":
        sys.argv.extend(["--yes", "--working-tree", "--index-only", "--no-workspace", "--no-agents", "--no-cost-tracking"])
    configure_runtime()
    from repowise.cli.main import cli
    cli()


if __name__ == "__main__":
    main()
elif __name__ == "__mp_main__":
    configure_runtime()
