"""
Git info helper: returns commit hash and branch name if the project is a git repo.
"""
from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Optional

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger

logger = get_logger()


def _run(cmd: list[str], cwd: Path) -> Optional[str]:
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=10,
            cwd=str(cwd),
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except Exception as exc:
        logger.debug(f"git command {cmd} failed: {exc}")
    return None


def get_commit_hash(short: bool = True) -> Optional[str]:
    fmt = "--short" if short else None
    cmd = ["git", "rev-parse"]
    if fmt:
        cmd.append(fmt)
    cmd.append("HEAD")
    return _run(cmd, PROJECT_ROOT)


def get_branch() -> Optional[str]:
    return _run(["git", "rev-parse", "--abbrev-ref", "HEAD"], PROJECT_ROOT)


def get_status() -> Optional[str]:
    return _run(["git", "status", "--short"], PROJECT_ROOT)


def get_info() -> dict[str, Optional[str]]:
    """Return a dict with git metadata; all values may be None if not a git repo."""
    commit = get_commit_hash(short=True)
    return {
        "commit_hash": commit,
        "branch": get_branch(),
        "dirty": (get_status() or "").strip() != "" if commit else None,
        "is_git_repo": commit is not None,
    }
