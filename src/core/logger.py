"""
Project logger: writes to logs/ directory with rotating file handler.
"""
from __future__ import annotations

import logging
import sys
from logging.handlers import RotatingFileHandler
from pathlib import Path

from src.core.config_loader import PROJECT_ROOT, ConfigLoader

_LOGS_DIR = PROJECT_ROOT / "logs"
_LOGS_DIR.mkdir(parents=True, exist_ok=True)

_initialized: set[str] = set()


def get_logger(name: str = "fire-detection", level: str | None = None) -> logging.Logger:
    """Return a configured logger that writes to both console and logs/app.log.

    Safe to call multiple times with the same name — handlers are added only once.
    """
    logger = logging.getLogger(name)

    if name in _initialized:
        return logger

    # Determine level
    if level is None:
        try:
            cfg = ConfigLoader()
            level = cfg.app.get("logging_level", "INFO")
        except Exception:
            level = "INFO"

    numeric_level = getattr(logging, level.upper(), logging.INFO)
    logger.setLevel(numeric_level)

    formatter = logging.Formatter(
        fmt="%(asctime)s | %(levelname)-8s | %(name)s | %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    # Console handler — RotatingFileHandler is a subclass of StreamHandler,
    # so we must exclude it explicitly to avoid false-positive matches.
    has_console = any(
        type(h) is logging.StreamHandler and getattr(h, "stream", None) is sys.stdout
        for h in logger.handlers
    )
    if not has_console:
        ch = logging.StreamHandler(sys.stdout)
        ch.setLevel(numeric_level)
        ch.setFormatter(formatter)
        logger.addHandler(ch)

    # Rotating file handler (10 MB × 3 backups) — guard with exact type check
    has_file = any(type(h) is RotatingFileHandler for h in logger.handlers)
    if not has_file:
        log_file = _LOGS_DIR / "app.log"
        fh = RotatingFileHandler(
            log_file, maxBytes=10 * 1024 * 1024, backupCount=3, encoding="utf-8"
        )
        fh.setLevel(numeric_level)
        fh.setFormatter(formatter)
        logger.addHandler(fh)

    logger.propagate = False
    _initialized.add(name)
    return logger
