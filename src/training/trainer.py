"""
FireDetectionTrainer — wraps Ultralytics YOLO.train() and streams
per-epoch metrics to an active MLflow run.

Usage (from pipeline stage):
    trainer = FireDetectionTrainer(cfg_loader)
    results = trainer.train(mlflow_client)
"""
from __future__ import annotations

import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import mlflow
import yaml

from src.core.config_loader import ConfigLoader, PROJECT_ROOT
from src.core.logger import get_logger
from src.model.loader import load_model

logger = get_logger()


class FireDetectionTrainer:
    """Orchestrates a YOLO fine-tuning run with MLflow metric logging."""

    def __init__(self, config_loader: ConfigLoader | None = None) -> None:
        self.cfg = config_loader or ConfigLoader()
        self._train_cfg: dict[str, Any] = self.cfg.load("training.yaml")

    # ------------------------------------------------------------------
    # Public
    # ------------------------------------------------------------------

    def train(self, run_name: str | None = None) -> dict[str, Any]:
        """Run fine-tuning; return final metrics dict.

        Must be called while an MLflow run is active (started by TrainStage).
        Set resume: true in training.yaml to continue from the last checkpoint.
        """
        train_cfg = self._train_cfg
        model_cfg = self.cfg.model
        dataset_cfg = self.cfg.dataset

        project_dir = PROJECT_ROOT / train_cfg.get("project", "runs/train")

        # ── Resume logic ────────────────────────────────────────────────────
        resume = bool(train_cfg.get("resume", False))
        weights = PROJECT_ROOT / model_cfg["init_weights_local_path"]

        if resume:
            resume_from = train_cfg.get("resume_from") or None
            if resume_from:
                checkpoint = PROJECT_ROOT / resume_from
            else:
                checkpoint = self._find_latest_checkpoint(project_dir)

            if checkpoint and checkpoint.exists():
                weights = checkpoint
                logger.info(f"Resuming training from checkpoint: {checkpoint}")
            else:
                logger.warning(
                    "resume=true but no checkpoint found in %s — "
                    "starting from init weights instead.", project_dir
                )
                resume = False
        # ────────────────────────────────────────────────────────────────────

        data_yaml = self._make_abs_data_yaml(
            PROJECT_ROOT / dataset_cfg["dataset_path"] / dataset_cfg.get("data_yaml_name", "data.yaml")
        )

        epochs  = int(train_cfg.get("epochs", 5))
        batch   = int(train_cfg.get("batch", 4))
        imgsz   = int(train_cfg.get("imgsz", 640))
        device  = str(train_cfg.get("device", "0"))
        workers = int(train_cfg.get("workers", 4))
        freeze  = train_cfg.get("freeze", 10)

        run_label = run_name or train_cfg.get("run_name", "dfire_finetune")
        ts = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        exp_name = f"{run_label}_{ts}"

        logger.info(
            "Training config: epochs=%d batch=%d imgsz=%d freeze=%s device=%s resume=%s",
            epochs, batch, imgsz, freeze, device, resume,
        )

        model = load_model(weights)
        self._disable_ultralytics_mlflow_callback(model)
        model.add_callback("on_fit_epoch_end", self._make_epoch_callback())

        # Log hyperparameters to active MLflow run
        mlflow.log_params({
            "epochs":        epochs,
            "batch":         batch,
            "imgsz":         imgsz,
            "freeze":        freeze,
            "optimizer":     train_cfg.get("optimizer", "AdamW"),
            "lr0":           train_cfg.get("lr0", 0.0005),
            "lrf":           train_cfg.get("lrf", 0.01),
            "warmup_epochs": train_cfg.get("warmup_epochs", 2.0),
            "patience":      train_cfg.get("patience", 20),
            "device":        device,
            "weights":       str(weights.relative_to(PROJECT_ROOT)),
            "dataset":       dataset_cfg.get("dataset_name", dataset_cfg.get("dataset_path")),
            "resumed":       resume,
        })

        logger.info(f"Starting YOLO.train() → {project_dir / exp_name}")

        # When resume=True, Ultralytics restores optimizer state + epoch count from
        # last.pt automatically; most other args below are ignored by Ultralytics
        # but we still pass them so the MLflow params above remain accurate.
        results = model.train(
            data=str(data_yaml),
            epochs=epochs,
            batch=batch,
            imgsz=imgsz,
            device=device,
            workers=workers,
            freeze=freeze,
            optimizer=str(train_cfg.get("optimizer", "AdamW")),
            lr0=float(train_cfg.get("lr0", 0.0005)),
            lrf=float(train_cfg.get("lrf", 0.01)),
            momentum=float(train_cfg.get("momentum", 0.937)),
            weight_decay=float(train_cfg.get("weight_decay", 0.0005)),
            warmup_epochs=float(train_cfg.get("warmup_epochs", 2.0)),
            warmup_momentum=float(train_cfg.get("warmup_momentum", 0.8)),
            patience=int(train_cfg.get("patience", 20)),
            hsv_h=float(train_cfg.get("hsv_h", 0.015)),
            hsv_s=float(train_cfg.get("hsv_s", 0.7)),
            hsv_v=float(train_cfg.get("hsv_v", 0.4)),
            flipud=float(train_cfg.get("flipud", 0.0)),
            fliplr=float(train_cfg.get("fliplr", 0.5)),
            mosaic=float(train_cfg.get("mosaic", 1.0)),
            mixup=float(train_cfg.get("mixup", 0.0)),
            save_period=int(train_cfg.get("save_period", -1)),
            project=str(project_dir),
            name=exp_name,
            exist_ok=False,
            resume=resume,
            verbose=True,
        )

        # Log final summary metrics
        final_metrics = self._extract_final_metrics(results)
        if final_metrics:
            mlflow.log_metrics(final_metrics)
            logger.info(f"Final metrics: {final_metrics}")

        # Log best weights as artifact
        best_pt = project_dir / exp_name / "weights" / "best.pt"
        if best_pt.exists():
            mlflow.log_artifact(str(best_pt), artifact_path="weights")
            logger.info(f"Best model logged: {best_pt}")

        # Log training plots
        for plot_name in ("results.png", "confusion_matrix.png", "F1_curve.png", "PR_curve.png"):
            plot_path = project_dir / exp_name / plot_name
            if plot_path.exists():
                mlflow.log_artifact(str(plot_path), artifact_path="plots")

        return final_metrics

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _find_latest_checkpoint(project_dir: Path) -> Path | None:
        """Return the most recently modified last.pt under project_dir, or None."""
        candidates = sorted(
            project_dir.glob("*/weights/last.pt"),
            key=lambda p: p.stat().st_mtime,
            reverse=True,
        )
        return candidates[0] if candidates else None

    def _make_abs_data_yaml(self, data_yaml_path: Path) -> Path:
        """Return a temp data.yaml where 'path' is resolved to absolute.

        The 'path' field in our data.yaml is relative to PROJECT_ROOT (e.g. 'data/DFire'),
        NOT relative to the yaml file itself.  Ultralytics resolves it relative to
        the yaml's parent, so we must replace it with the true absolute path before
        passing it to YOLO.train().
        """
        with open(data_yaml_path, "r", encoding="utf-8") as fh:
            data = yaml.safe_load(fh) or {}

        raw_path = data.get("path", "")
        if raw_path:
            candidate = Path(raw_path)
            if candidate.is_absolute():
                resolved = candidate
            else:
                # raw_path is relative to PROJECT_ROOT (e.g. "data/DFire")
                resolved = (PROJECT_ROOT / raw_path).resolve()
        else:
            resolved = data_yaml_path.parent.resolve()

        data["path"] = str(resolved).replace("\\", "/")

        tmp = tempfile.NamedTemporaryFile(
            suffix=".yaml", delete=False, mode="w", encoding="utf-8", prefix="dfire_train_"
        )
        yaml.dump(data, tmp, sort_keys=False, allow_unicode=True)
        tmp.close()
        logger.debug(f"Resolved data.yaml written to {tmp.name}  (path={data['path']})")
        return Path(tmp.name)

    def _make_epoch_callback(self):
        """Return an on_fit_epoch_end callback that logs metrics to the active MLflow run."""
        def on_fit_epoch_end(trainer) -> None:
            epoch = getattr(trainer, "epoch", 0)
            raw = {str(k): float(v) for k, v in (trainer.metrics or {}).items()}

            # Remap keys (replace slashes, strip (B) suffix)
            clean = {}
            for raw_k, raw_v in raw.items():
                clean_k = raw_k.replace("/", ".").replace("(B)", "")
                clean[clean_k] = raw_v

            if clean:
                mlflow.log_metrics(clean, step=epoch)

        return on_fit_epoch_end

    @staticmethod
    def _disable_ultralytics_mlflow_callback(model) -> None:
        """Remove Ultralytics' built-in MLflow callback to avoid duplicate runs."""
        try:
            callbacks = model.callbacks
            for event in list(callbacks.keys()):
                callbacks[event] = [
                    cb for cb in callbacks[event]
                    if getattr(cb, "__module__", "").find("mlflow") == -1
                ]
        except Exception:
            pass  # Non-fatal; worst case both callbacks run

    @staticmethod
    def _extract_final_metrics(results) -> dict[str, float]:
        """Pull mAP50 / mAP50-95 from Ultralytics Results object."""
        try:
            box = results.results_dict
            return {
                k.replace("/", ".").replace("(B)", ""): float(v)
                for k, v in box.items()
                if isinstance(v, (int, float))
            }
        except Exception:
            return {}
