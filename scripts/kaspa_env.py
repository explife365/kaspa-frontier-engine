"""Load and upsert secrets from repo-root kaspa.env (gitignored)."""

from __future__ import annotations

import os
import subprocess
import tempfile
from contextlib import contextmanager
from pathlib import Path

ENV_NAME = "kaspa.env"


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def env_path(root: Path | None = None) -> Path:
    return (root or repo_root()) / ENV_NAME


@contextmanager
def _env_lock(path: Path):
    lock_path = path.with_suffix(path.suffix + ".lock")
    with lock_path.open("a+b") as handle:
        if os.name == "nt":
            import msvcrt

            handle.seek(0, os.SEEK_END)
            if handle.tell() == 0:
                handle.write(b"\0")
                handle.flush()
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_LOCK, 1)
            try:
                yield
            finally:
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl

            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def _secure_permissions(path: Path) -> None:
    if os.name != "nt":
        path.chmod(0o600)
        return
    username = os.environ.get("USERNAME")
    domain = os.environ.get("USERDOMAIN")
    if not username:
        raise RuntimeError("cannot secure kaspa.env: USERNAME is unavailable")
    identity = f"{domain}\\{username}" if domain else username
    result = subprocess.run(
        [
            "icacls",
            str(path),
            "/inheritance:r",
            "/grant:r",
            f"{identity}:(F)",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError("cannot apply current-user-only ACL to kaspa.env")


def load_kaspa_env(root: Path | None = None) -> Path:
    """Apply kaspa.env into os.environ. Existing process env wins."""
    path = env_path(root)
    if not path.is_file():
        return path
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key and key not in os.environ:
            os.environ[key] = value
    return path


def upsert_kaspa_env(updates: dict[str, str], root: Path | None = None) -> Path:
    """Write or replace keys in kaspa.env. Preserves comments. Does not print values."""
    path = env_path(root)
    path.parent.mkdir(parents=True, exist_ok=True)
    with _env_lock(path):
        lines: list[str]
        if path.is_file():
            lines = path.read_text(encoding="utf-8").splitlines()
        else:
            lines = [
                "# Local secrets. Never commit. Copy from kaspa.env.example.",
                "",
            ]

        found: set[str] = set()
        rewritten: list[str] = []
        for raw in lines:
            stripped = raw.strip()
            if stripped and not stripped.startswith("#") and "=" in stripped:
                key, _, _ = stripped.partition("=")
                key = key.strip()
                if key in updates:
                    rewritten.append(f"{key}={updates[key]}")
                    found.add(key)
                    continue
            rewritten.append(raw.rstrip("\n"))
        for key, value in updates.items():
            if key not in found:
                rewritten.append(f"{key}={value}")

        with tempfile.NamedTemporaryFile(
            "w",
            encoding="utf-8",
            dir=path.parent,
            prefix=f".{path.name}.",
            delete=False,
        ) as handle:
            handle.write("\n".join(rewritten) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
            temp_path = Path(handle.name)
        try:
            _secure_permissions(temp_path)
            os.replace(temp_path, path)
        finally:
            if temp_path.exists():
                temp_path.unlink()
    return path
