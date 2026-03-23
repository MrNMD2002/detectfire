"""
Experiment manager: higher-level helpers for querying MLflow runs.
"""
from __future__ import annotations

from typing import Optional

import mlflow
from mlflow.entities import Run
from mlflow.tracking import MlflowClient

from src.core.logger import get_logger
from src.tracking.mlflow_client import MLflowClient as ProjectMLflowClient

logger = get_logger()


class ExperimentManager:
    """Query and manage MLflow experiments and runs."""

    def __init__(self, project_client: ProjectMLflowClient | None = None) -> None:
        self._project_client = project_client or ProjectMLflowClient()
        self._project_client.configure()

    @property
    def _client(self) -> MlflowClient:
        return MlflowClient(tracking_uri=self._project_client.tracking_uri)

    def get_experiment_id(self) -> str | None:
        exp = mlflow.get_experiment_by_name(self._project_client.experiment_name)
        if exp is None:
            logger.warning(
                f"Experiment '{self._project_client.experiment_name}' not found in MLflow."
            )
            return None
        return exp.experiment_id

    def get_last_n_runs(self, n: int = 10) -> list[Run]:
        """Return the N most recent runs for the configured experiment."""
        exp_id = self.get_experiment_id()
        if exp_id is None:
            return []
        runs = self._client.search_runs(
            experiment_ids=[exp_id],
            order_by=["attributes.start_time DESC"],
            max_results=n,
        )
        return list(runs)

    def get_best_run(
        self, metric: str = "val_map50", ascending: bool = False
    ) -> Run | None:
        """Return the run with the best value for a given metric."""
        exp_id = self.get_experiment_id()
        if exp_id is None:
            return None
        order = "ASC" if ascending else "DESC"
        runs = self._client.search_runs(
            experiment_ids=[exp_id],
            filter_string=f"metrics.{metric} > 0",
            order_by=[f"metrics.{metric} {order}"],
            max_results=1,
        )
        return runs[0] if runs else None
