"""
Experiment summary: generates reports/experiments/experiment_summary.md
listing the last N MLflow runs with key params and metrics.
"""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger
from src.tracking.experiment_manager import ExperimentManager

logger = get_logger()

REPORT_DIR = PROJECT_ROOT / "reports" / "experiments"
REPORT_DIR.mkdir(parents=True, exist_ok=True)
OUTPUT_PATH = REPORT_DIR / "experiment_summary.md"

# Params and metrics to include in the summary table (in order)
SUMMARY_PARAMS = [
    "dataset_project",
    "dataset_version",
    "init_weights_source",
    "init_weights_local_path",
    "git_commit",
    "cuda_available",
    "gpu_name",
]
SUMMARY_METRICS = [
    "framework_smoke_metric",
    "val_map50",
    "val_map50_95",
    "train_loss",
]


def _truncate(val: str, max_len: int = 40) -> str:
    if len(val) > max_len:
        return val[: max_len - 3] + "..."
    return val


def _format_timestamp(ts_ms: Optional[int]) -> str:
    if ts_ms is None:
        return "—"
    dt = datetime.fromtimestamp(ts_ms / 1000, tz=timezone.utc)
    return dt.strftime("%Y-%m-%d %H:%M UTC")


def generate(n: int = 20, manager: Optional[ExperimentManager] = None) -> Path:
    """Query MLflow and write experiment_summary.md. Returns the output path."""
    if manager is None:
        manager = ExperimentManager()

    now = datetime.now(timezone.utc).isoformat()
    exp_name = manager._project_client.experiment_name

    lines: list[str] = [
        "# Experiment Summary",
        f"\n_Generated: {now}_  ",
        f"**Experiment:** `{exp_name}`  ",
    ]

    try:
        runs = manager.get_last_n_runs(n=n)
    except Exception as exc:
        logger.error(f"Could not fetch runs from MLflow: {exc}")
        lines += [
            "\n## Error\n",
            f"> Could not connect to MLflow: {exc}",
            "\nEnsure the MLflow server is running (`docker compose up -d` in `infra/mlflow/`).",
        ]
        OUTPUT_PATH.write_text("\n".join(lines) + "\n", encoding="utf-8")
        logger.info(f"Experiment summary written (with error) to {OUTPUT_PATH}")
        return OUTPUT_PATH

    if not runs:
        lines += [
            "\n## No Runs Found\n",
            f"No runs exist yet for experiment `{exp_name}`.",
            "\nRun the orchestrator first:",
            "```",
            "python -m src.pipeline.orchestrator",
            "```",
        ]
        OUTPUT_PATH.write_text("\n".join(lines) + "\n", encoding="utf-8")
        logger.info(f"Experiment summary written (no runs) to {OUTPUT_PATH}")
        return OUTPUT_PATH

    lines.append(f"\n**Total runs shown:** {len(runs)} (of last {n})\n")

    # Table header
    header_cols = ["Run ID", "Name", "Status", "Started", "Duration (s)"]
    for p in SUMMARY_PARAMS:
        header_cols.append(p)
    for m in SUMMARY_METRICS:
        header_cols.append(m)

    sep = [":---"] * len(header_cols)
    lines.append("| " + " | ".join(header_cols) + " |")
    lines.append("| " + " | ".join(sep) + " |")

    for run in runs:
        info = run.info
        params = run.data.params
        metrics = run.data.metrics

        # Duration
        if info.end_time and info.start_time:
            dur = round((info.end_time - info.start_time) / 1000, 1)
        else:
            dur = "—"

        row = [
            f"`{info.run_id[:8]}`",
            _truncate(info.run_name or "—", 30),
            info.status,
            _format_timestamp(info.start_time),
            str(dur),
        ]
        for p in SUMMARY_PARAMS:
            row.append(_truncate(params.get(p, "—"), 30))
        for m in SUMMARY_METRICS:
            val = metrics.get(m)
            row.append(str(round(val, 4)) if val is not None else "—")

        lines.append("| " + " | ".join(row) + " |")

    # Tags summary per run
    lines.append("\n## Tags\n")
    for run in runs:
        tags = run.data.tags
        filtered = {k: v for k, v in tags.items() if not k.startswith("mlflow.")}
        if filtered:
            tag_str = ", ".join(f"`{k}={v}`" for k, v in sorted(filtered.items()))
            lines.append(f"- `{run.info.run_id[:8]}` — {tag_str}")

    OUTPUT_PATH.write_text("\n".join(lines) + "\n", encoding="utf-8")
    logger.info(f"Experiment summary written to {OUTPUT_PATH} ({len(runs)} runs)")
    return OUTPUT_PATH
