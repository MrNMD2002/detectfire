"""
Config loader: loads YAML config files, validates required keys, no hardcoded paths.
"""
from __future__ import annotations

import os
from pathlib import Path
from typing import Any

import yaml

try:
    from dotenv import load_dotenv as _load_dotenv
    _DOTENV_AVAILABLE = True
except ImportError:
    _DOTENV_AVAILABLE = False


# Root of the project (two levels up from src/core/)
PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
CONFIG_DIR = PROJECT_ROOT / "config"

_REQUIRED_KEYS: dict[str, list[str]] = {
    "app.yaml": ["project_name", "environment", "logging_level", "seed"],
    # Only truly universal keys are required.
    # Roboflow-specific keys (dataset_project, dataset_version, dataset_format)
    # are optional and only present when dataset_source == "roboflow".
    "dataset.yaml": [
        "dataset_source",
        "dataset_path",
    ],
    "mlflow.yaml": ["tracking_uri", "experiment_name", "minio_endpoint", "minio_bucket"],
    "model.yaml": [
        "init_weights_source",
        "init_weights_repo",
        "init_weights_file",
        "init_weights_local_path",
        "model_family",
    ],
    "environment.yaml": ["python_version"],
}


class ConfigLoader:
    """Loads all YAML configs from config/ and exposes them as typed dicts."""

    def __init__(self, config_dir: Path | None = None) -> None:
        # Load .env from project root (ignored by git, safe for secrets).
        # Must run before any os.environ reads downstream.
        if _DOTENV_AVAILABLE:
            _env_file = PROJECT_ROOT / ".env"
            if _env_file.exists():
                _load_dotenv(_env_file, override=False)

        self._dir = Path(config_dir) if config_dir else CONFIG_DIR
        self._cache: dict[str, dict[str, Any]] = {}

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def load(self, filename: str) -> dict[str, Any]:
        """Load a single YAML file (filename relative to config_dir).

        Result is cached after first load.
        """
        if filename not in self._cache:
            path = self._dir / filename
            if not path.exists():
                raise FileNotFoundError(
                    f"Config file not found: {path}. "
                    f"Expected under {self._dir}/"
                )
            with open(path, "r", encoding="utf-8") as fh:
                data = yaml.safe_load(fh) or {}
            self._validate(filename, data)
            self._cache[filename] = data
        return self._cache[filename]

    def load_all(self) -> dict[str, dict[str, Any]]:
        """Load every YAML file in config_dir and return as a dict keyed by filename."""
        results: dict[str, dict[str, Any]] = {}
        for yaml_file in sorted(self._dir.glob("*.yaml")):
            results[yaml_file.name] = self.load(yaml_file.name)
        return results

    # Convenience properties -------------------------------------------

    @property
    def app(self) -> dict[str, Any]:
        return self.load("app.yaml")

    @property
    def dataset(self) -> dict[str, Any]:
        return self.load("dataset.yaml")

    @property
    def mlflow(self) -> dict[str, Any]:
        return self.load("mlflow.yaml")

    @property
    def model(self) -> dict[str, Any]:
        return self.load("model.yaml")

    @property
    def environment(self) -> dict[str, Any]:
        return self.load("environment.yaml")

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def resolve_path(self, relative: str) -> Path:
        """Resolve a path relative to project root."""
        return PROJECT_ROOT / relative

    def _validate(self, filename: str, data: dict[str, Any]) -> None:
        required = _REQUIRED_KEYS.get(filename, [])
        missing = [k for k in required if k not in data]
        if missing:
            raise KeyError(
                f"Config {filename} is missing required keys: {missing}"
            )
