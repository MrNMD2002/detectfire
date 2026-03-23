"""
Artifact manager: batch-upload evaluation artifacts to the active MLflow run.
"""
from __future__ import annotations

from pathlib import Path
from typing import Optional

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger
from src.tracking.mlflow_client import MLflowClient

logger = get_logger()

# Artifacts to log in the framework smoke run
FRAMEWORK_ARTIFACTS = [
    PROJECT_ROOT / "reports" / "evaluation" / "dataset_report.md",
    PROJECT_ROOT / "reports" / "evaluation" / "dataset_stats.json",
    PROJECT_ROOT / "reports" / "evaluation" / "env_fingerprint.json",
    PROJECT_ROOT / "reports" / "evaluation" / "init_weights_manifest.json",
]


class ArtifactManager:
    """Manages artifact uploads to the active MLflow run."""

    def __init__(self, client: MLflowClient | None = None) -> None:
        self._client = client or MLflowClient()

    def log_framework_artifacts(self) -> list[Path]:
        """Log all framework evaluation artifacts. Returns list of logged paths."""
        logged: list[Path] = []
        for path in FRAMEWORK_ARTIFACTS:
            if path.exists():
                self._client.log_artifact(path)
                logged.append(path)
            else:
                logger.warning(f"Framework artifact missing, skipping: {path}")
        logger.info(f"Framework artifacts logged: {len(logged)}/{len(FRAMEWORK_ARTIFACTS)}")
        return logged

    def log_file(self, path: Path, artifact_subdir: str | None = None) -> None:
        self._client.log_artifact(path, artifact_path=artifact_subdir)

    def log_directory(self, directory: Path, artifact_subdir: str | None = None) -> None:
        self._client.log_artifacts(directory, artifact_path=artifact_subdir)
