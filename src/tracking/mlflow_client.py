"""
MLflow client wrapper: reads config/mlflow.yaml, sets up tracking URI, experiment,
and provides run management helpers.
"""
from __future__ import annotations

import os
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Optional

import mlflow
from mlflow.entities import Run

from src.core.config_loader import ConfigLoader
from src.core.logger import get_logger

logger = get_logger()


class MLflowClient:
    """Thin wrapper around the MLflow Python SDK.

    Reads mlflow.yaml for all configuration; nothing is hardcoded.
    """

    def __init__(self, config_loader: ConfigLoader | None = None) -> None:
        self._cfg_loader = config_loader or ConfigLoader()
        self._cfg = self._cfg_loader.mlflow
        self._active_run: Run | None = None
        self._configured = False

    # ------------------------------------------------------------------
    # Setup
    # ------------------------------------------------------------------

    def configure(self) -> None:
        """Apply MLflow + MinIO environment variables from config."""
        if self._configured:
            return

        tracking_uri = self._cfg["tracking_uri"]
        # Set both the Python API and the env var so that subprocesses /
        # third-party integrations (e.g. ultralytics YOLO) pick up the URI.
        os.environ["MLFLOW_TRACKING_URI"] = tracking_uri
        mlflow.set_tracking_uri(tracking_uri)
        logger.info(f"MLflow tracking URI: {tracking_uri}")

        # MinIO / S3 credentials for artifact store.
        # Priority: env var (from .env or CI/CD) → dev fallback.
        # Never hardcode real credentials here — set MINIO_ACCESS_KEY / MINIO_SECRET_KEY in .env.
        minio_endpoint = self._cfg.get("minio_endpoint", "")
        if minio_endpoint:
            os.environ.setdefault("MLFLOW_S3_ENDPOINT_URL", minio_endpoint)
            access_key = os.environ.get("MINIO_ACCESS_KEY", "minioadmin")
            secret_key = os.environ.get("MINIO_SECRET_KEY", "minioadmin")
            os.environ.setdefault("AWS_ACCESS_KEY_ID", access_key)
            os.environ.setdefault("AWS_SECRET_ACCESS_KEY", secret_key)
            logger.debug(f"MinIO endpoint set: {minio_endpoint}")

        self._configured = True

    def set_experiment(self) -> mlflow.entities.Experiment:
        """Create or retrieve the experiment specified in mlflow.yaml."""
        self.configure()
        exp_name = self._cfg["experiment_name"]
        experiment = mlflow.set_experiment(exp_name)
        logger.info(f"Experiment set: '{exp_name}' (id={experiment.experiment_id})")
        return experiment

    # ------------------------------------------------------------------
    # Run helpers
    # ------------------------------------------------------------------

    def start_run(
        self,
        run_name: str | None = None,
        tags: dict[str, str] | None = None,
        nested: bool = False,
    ) -> mlflow.ActiveRun:
        self.set_experiment()
        run = mlflow.start_run(run_name=run_name, tags=tags, nested=nested)
        self._active_run = run.info
        logger.info(f"MLflow run started: {run.info.run_id} name={run_name!r}")
        return run

    def end_run(self, status: str = "FINISHED") -> None:
        mlflow.end_run(status=status)
        logger.info(f"MLflow run ended (status={status})")

    @contextmanager
    def run(
        self,
        run_name: str | None = None,
        tags: dict[str, str] | None = None,
    ) -> Generator[mlflow.ActiveRun, None, None]:
        """Context manager: start/end a run automatically."""
        active = self.start_run(run_name=run_name, tags=tags)
        try:
            yield active
            self.end_run("FINISHED")
        except Exception as exc:
            logger.error(f"Run failed: {exc}")
            self.end_run("FAILED")
            raise

    # ------------------------------------------------------------------
    # Logging helpers
    # ------------------------------------------------------------------

    def log_params(self, params: dict[str, Any]) -> None:
        """Log a dict of params, converting values to strings."""
        safe = {k: str(v) if v is not None else "" for k, v in params.items()}
        mlflow.log_params(safe)
        logger.debug(f"Logged {len(safe)} params")

    def log_metrics(self, metrics: dict[str, float], step: int | None = None) -> None:
        mlflow.log_metrics(metrics, step=step)
        logger.debug(f"Logged metrics: {metrics}")

    def log_artifact(self, local_path: str | Path, artifact_path: str | None = None) -> None:
        path = Path(local_path)
        if not path.exists():
            logger.warning(f"Artifact not found, skipping: {path}")
            return
        mlflow.log_artifact(str(path), artifact_path=artifact_path)
        logger.info(f"Artifact logged: {path.name}")

    def log_artifacts(self, local_dir: str | Path, artifact_path: str | None = None) -> None:
        d = Path(local_dir)
        if not d.exists():
            logger.warning(f"Artifact directory not found, skipping: {d}")
            return
        mlflow.log_artifacts(str(d), artifact_path=artifact_path)
        logger.info(f"Artifacts logged from: {d}")

    def set_tags(self, tags: dict[str, str]) -> None:
        mlflow.set_tags(tags)
        logger.debug(f"Tags set: {tags}")

    # ------------------------------------------------------------------
    # Query helpers
    # ------------------------------------------------------------------

    @property
    def experiment_name(self) -> str:
        return self._cfg["experiment_name"]

    @property
    def tracking_uri(self) -> str:
        return self._cfg["tracking_uri"]
