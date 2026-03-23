"""
Environment fingerprint: captures OS, Python, pip packages, GPU info.
Exports to reports/evaluation/env_fingerprint.json.
"""
from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger

logger = get_logger()

REPORT_DIR = PROJECT_ROOT / "reports" / "evaluation"
REPORT_DIR.mkdir(parents=True, exist_ok=True)
OUTPUT_PATH = REPORT_DIR / "env_fingerprint.json"


def _get_pip_packages() -> list[dict[str, str]]:
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pip", "list", "--format=json"],
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode == 0:
            packages = json.loads(result.stdout)
            return [{"name": p["name"], "version": p["version"]} for p in packages]
    except Exception as exc:
        logger.warning(f"Could not enumerate pip packages: {exc}")
    return []


def _get_gpu_info() -> dict[str, Any]:
    info: dict[str, Any] = {"cuda_available": False, "gpu_name": None, "gpu_count": 0}
    try:
        import torch  # type: ignore
        info["cuda_available"] = torch.cuda.is_available()
        if info["cuda_available"]:
            info["gpu_count"] = torch.cuda.device_count()
            info["gpu_name"] = torch.cuda.get_device_name(0)
            info["cuda_version"] = torch.version.cuda
    except ImportError:
        # Try nvidia-smi as fallback
        try:
            result = subprocess.run(
                ["nvidia-smi", "--query-gpu=name", "--format=csv,noheader"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode == 0:
                gpus = [line.strip() for line in result.stdout.strip().splitlines() if line.strip()]
                if gpus:
                    info["gpu_name"] = gpus[0]
                    info["gpu_count"] = len(gpus)
                    info["cuda_available"] = True
        except Exception:
            pass
    except Exception as exc:
        logger.warning(f"Could not query GPU info: {exc}")
    return info


def collect() -> dict[str, Any]:
    """Collect and return the environment fingerprint."""
    gpu = _get_gpu_info()
    fingerprint: dict[str, Any] = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "platform": {
            "system": platform.system(),
            "release": platform.release(),
            "version": platform.version(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "node": platform.node(),
        },
        "python": {
            "version": platform.python_version(),
            "implementation": platform.python_implementation(),
            "executable": sys.executable,
        },
        "gpu": gpu,
        "pip_packages": _get_pip_packages(),
        "env_vars": {
            k: v for k, v in os.environ.items()
            if k.startswith(("MLFLOW", "AWS", "CUDA", "TORCH", "PYTHONPATH"))
        },
    }
    return fingerprint


def export(fingerprint: dict[str, Any] | None = None) -> Path:
    """Collect (or accept pre-collected) fingerprint, write JSON, return path."""
    if fingerprint is None:
        fingerprint = collect()
    OUTPUT_PATH.write_text(
        json.dumps(fingerprint, indent=2, default=str),
        encoding="utf-8",
    )
    logger.info(f"Env fingerprint written to {OUTPUT_PATH}")
    return OUTPUT_PATH
