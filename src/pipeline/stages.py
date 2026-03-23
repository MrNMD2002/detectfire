"""
Pipeline stage definitions.

Each stage:
  - Receives a shared context dict
  - Mutates it with its outputs
  - Returns True on success, False on non-fatal failure
  - Raises on fatal errors

Stages are ordered and run by the Orchestrator.
"""
from __future__ import annotations

import hashlib
import json
from abc import ABC, abstractmethod
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from src.core.config_loader import ConfigLoader, PROJECT_ROOT
from src.core.logger import get_logger

logger = get_logger()


# ---------------------------------------------------------------------------
# Base class
# ---------------------------------------------------------------------------

class BaseStage(ABC):
    name: str = "base"

    def __init__(self, cfg: ConfigLoader) -> None:
        self.cfg = cfg

    @abstractmethod
    def run(self, ctx: dict[str, Any]) -> bool:
        ...

    def _log(self, msg: str) -> None:
        logger.info(f"[{self.name}] {msg}")

    def _warn(self, msg: str) -> None:
        logger.warning(f"[{self.name}] {msg}")

    def _error(self, msg: str) -> None:
        logger.error(f"[{self.name}] {msg}")


# ---------------------------------------------------------------------------
# Stage 1 – LoadConfigStage
# ---------------------------------------------------------------------------

class LoadConfigStage(BaseStage):
    name = "LoadConfig"

    def run(self, ctx: dict[str, Any]) -> bool:
        self._log("Loading all config files …")
        ctx["app_cfg"] = self.cfg.app
        ctx["dataset_cfg"] = self.cfg.dataset
        ctx["mlflow_cfg"] = self.cfg.mlflow
        ctx["model_cfg"] = self.cfg.model
        ctx["env_cfg"] = self.cfg.environment
        self._log(
            f"project={ctx['app_cfg']['project_name']} "
            f"env={ctx['app_cfg']['environment']}"
        )
        return True


# ---------------------------------------------------------------------------
# Stage 2 – EnvFingerprintStage
# ---------------------------------------------------------------------------

class EnvFingerprintStage(BaseStage):
    name = "EnvFingerprint"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.core import env_fingerprint
        from src.core import git_info

        self._log("Collecting environment fingerprint …")
        fp = env_fingerprint.collect()

        # Attach git info
        git = git_info.get_info()
        fp["git"] = git
        ctx["git_info"] = git

        path = env_fingerprint.export(fp)
        ctx["env_fingerprint"] = fp
        ctx["env_fingerprint_path"] = path
        self._log(f"Written to {path}")
        return True


# ---------------------------------------------------------------------------
# Stage 3 – InitWeightsManifestStage
# ---------------------------------------------------------------------------

class InitWeightsManifestStage(BaseStage):
    name = "InitWeightsManifest"

    def run(self, ctx: dict[str, Any]) -> bool:
        model_cfg = ctx.get("model_cfg", self.cfg.model)
        local_path = PROJECT_ROOT / model_cfg["init_weights_local_path"]

        self._log(f"Checking init weights at {local_path} …")
        present = local_path.exists()
        size_bytes: int = 0
        sha256: str | None = None

        if present:
            size_bytes = local_path.stat().st_size
            self._log(f"File found ({size_bytes / 1e6:.1f} MB). Computing sha256 …")
            try:
                h = hashlib.sha256()
                with open(local_path, "rb") as fh:
                    for chunk in iter(lambda: fh.read(65536), b""):
                        h.update(chunk)
                sha256 = h.hexdigest()
                self._log(f"sha256={sha256}")
            except Exception as exc:
                self._warn(f"sha256 computation failed: {exc}")
        else:
            self._warn(f"Init weights NOT found at {local_path}")

        manifest = {
            "generated_utc": datetime.now(timezone.utc).isoformat(),
            "source": model_cfg.get("init_weights_source"),
            "repo": model_cfg.get("init_weights_repo"),
            "file": model_cfg.get("init_weights_file"),
            "local_path": str(local_path),
            "present": present,
            "size_bytes": size_bytes,
            "sha256": sha256,
            "model_family": model_cfg.get("model_family"),
            "note": model_cfg.get("note"),
        }

        out_path = PROJECT_ROOT / "reports" / "evaluation" / "init_weights_manifest.json"
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        self._log(f"Manifest written to {out_path}")

        ctx["init_weights_manifest"] = manifest
        ctx["init_weights_manifest_path"] = out_path
        ctx["init_weights_present"] = present
        ctx["init_weights_sha256"] = sha256
        return True


# ---------------------------------------------------------------------------
# Stage 4 – DatasetValidateStage
# ---------------------------------------------------------------------------

class DatasetValidateStage(BaseStage):
    name = "DatasetValidate"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.dataset.metadata import load_metadata
        from src.dataset.validator import validate

        dataset_cfg = ctx.get("dataset_cfg", self.cfg.dataset)
        dataset_path = PROJECT_ROOT / dataset_cfg["dataset_path"]

        self._log(f"Validating dataset at {dataset_path} …")
        meta = load_metadata(dataset_path)
        ctx["dataset_meta"] = meta

        if not meta.loaded:
            self._warn(f"Dataset not available: {meta.error}")
            ctx["validation_report"] = None
            return True  # Non-fatal

        report = validate(meta)
        ctx["validation_report"] = report
        self._log(report.summary())
        return True


# ---------------------------------------------------------------------------
# Stage 5 – DatasetAnalyzeStage
# ---------------------------------------------------------------------------

class DatasetAnalyzeStage(BaseStage):
    name = "DatasetAnalyze"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.dataset.analyzer import analyze

        meta = ctx.get("dataset_meta")
        if meta is None or not meta.loaded:
            self._warn("Dataset metadata not available, skipping analysis.")
            ctx["dataset_stats"] = None
            return True  # Non-fatal

        self._log("Analyzing dataset …")
        stats = analyze(meta)
        ctx["dataset_stats"] = stats
        self._log(
            f"total_images={stats.total_images} total_boxes={stats.total_boxes}"
        )
        return True


# ---------------------------------------------------------------------------
# Stage 6 – DatasetReportExportStage
# ---------------------------------------------------------------------------

class DatasetReportExportStage(BaseStage):
    name = "DatasetReportExport"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.dataset.report_exporter import export

        meta = ctx.get("dataset_meta")
        validation = ctx.get("validation_report")
        stats = ctx.get("dataset_stats")

        if meta is None:
            # Create a minimal placeholder metadata
            from src.dataset.metadata import DatasetMetadata
            from src.core.config_loader import ConfigLoader
            cfg = self.cfg.dataset
            dataset_path = PROJECT_ROOT / cfg["dataset_path"]
            meta = DatasetMetadata(
                data_yaml_path=dataset_path / "data.yaml",
                dataset_root=dataset_path,
                error="Dataset metadata was not collected.",
            )

        self._log("Exporting dataset report …")
        md_path, json_path = export(meta, validation, stats)
        ctx["dataset_report_md"] = md_path
        ctx["dataset_stats_json"] = json_path
        self._log(f"Report: {md_path}")
        self._log(f"Stats:  {json_path}")
        return True


# ---------------------------------------------------------------------------
# Stage 7 – MLflowSmokeRunStage
# ---------------------------------------------------------------------------

class MLflowSmokeRunStage(BaseStage):
    name = "MLflowSmokeRun"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.tracking.mlflow_client import MLflowClient
        from src.tracking.artifact_manager import ArtifactManager

        self._log("Starting MLflow framework smoke run …")
        client = MLflowClient(config_loader=self.cfg)

        dataset_cfg = ctx.get("dataset_cfg", self.cfg.dataset)
        model_cfg = ctx.get("model_cfg", self.cfg.model)
        git_info = ctx.get("git_info", {})
        fp = ctx.get("env_fingerprint", {})
        gpu = fp.get("gpu", {})

        params = {
            # Dataset (generic keys — work for both Roboflow and local datasets)
            "dataset_source":  dataset_cfg.get("dataset_source"),
            "dataset_name":    dataset_cfg.get("dataset_name") or dataset_cfg.get("dataset_project"),
            "dataset_version": dataset_cfg.get("dataset_version"),
            "dataset_path":    dataset_cfg.get("dataset_path"),
            # Weights
            "init_weights_source": model_cfg.get("init_weights_source"),
            "init_weights_repo": model_cfg.get("init_weights_repo"),
            "init_weights_local_path": model_cfg.get("init_weights_local_path"),
            "init_weights_present": ctx.get("init_weights_present", False),
            "init_weights_sha256": ctx.get("init_weights_sha256", ""),
            # Git
            "git_commit": git_info.get("commit_hash", ""),
            # GPU
            "cuda_available": gpu.get("cuda_available", False),
            "gpu_name": gpu.get("gpu_name", ""),
        }

        metrics = {
            "framework_smoke_metric": 1.0,
        }

        tags = {
            "phase": "framework",
            "stage": "smoke",
        }

        try:
            with client.run(run_name="framework-smoke", tags=tags):
                client.log_params(params)
                client.log_metrics(metrics)

                am = ArtifactManager(client=client)
                am.log_framework_artifacts()

            self._log("Smoke run completed successfully.")
            return True
        except Exception as exc:
            self._error(f"MLflow smoke run failed: {exc}")
            self._error(
                "Is MLflow server running? "
                "Run: cd infra/mlflow && docker compose up -d"
            )
            ctx["mlflow_smoke_error"] = str(exc)
            return False  # Non-fatal to pipeline


# ---------------------------------------------------------------------------
# Stage 10 – ExperimentSummaryExportStage
# ---------------------------------------------------------------------------

class ExperimentSummaryExportStage(BaseStage):
    name = "ExperimentSummaryExport"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.reporting import experiment_summary

        self._log("Generating experiment summary report …")
        try:
            path = experiment_summary.generate(n=20)
            ctx["experiment_summary_path"] = path
            self._log(f"Summary written to {path}")
            return True
        except Exception as exc:
            self._warn(f"Experiment summary failed (non-fatal): {exc}")
            return True  # Non-fatal


# ---------------------------------------------------------------------------
# Stage 8 – TrainStage
# ---------------------------------------------------------------------------

class TrainStage(BaseStage):
    name = "Train"

    def run(self, ctx: dict[str, Any]) -> bool:
        from src.tracking.mlflow_client import MLflowClient
        from src.training.trainer import FireDetectionTrainer
        from datetime import datetime, timezone

        train_cfg = self.cfg.load("training.yaml")

        if not train_cfg.get("enabled", True):
            self._log("Training disabled (enabled: false in training.yaml). Skipping.")
            ctx["train_skipped"] = True
            return True

        self._log(
            f"Starting fine-tuning  epochs={train_cfg.get('epochs')} "
            f"batch={train_cfg.get('batch')} freeze={train_cfg.get('freeze')}"
        )

        client = MLflowClient(config_loader=self.cfg)
        trainer = FireDetectionTrainer(config_loader=self.cfg)

        ts = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        run_name = f"{train_cfg.get('run_name', 'dfire_finetune')}_{ts}"

        tags = {
            "phase": "train",
            "stage": "finetune",
            "dataset": self.cfg.dataset.get("dataset_name", "unknown"),
            "freeze_layers": str(train_cfg.get("freeze", 10)),
        }

        try:
            with client.run(run_name=run_name, tags=tags):
                self._log(f"MLflow run: {run_name}")
                final_metrics = trainer.train(run_name=run_name)
                ctx["train_metrics"] = final_metrics
                ctx["train_run_name"] = run_name

            map50 = final_metrics.get("metrics.mAP50", final_metrics.get("metrics/mAP50(B)", "?"))
            self._log(f"Training complete. Best mAP50={map50}")
            return True

        except Exception as exc:
            self._error(f"Training failed: {exc}")
            ctx["train_error"] = str(exc)
            return False


# ---------------------------------------------------------------------------
# Stage 10 – ModelEvaluateStage
# ---------------------------------------------------------------------------

class ModelEvaluateStage(BaseStage):
    name = "ModelEvaluate"

    def run(self, ctx: dict[str, Any]) -> bool:
        """Run YOLO val() on the eval split, log per-class metrics, export reference data."""
        try:
            from ultralytics import YOLO
        except ImportError:
            self._warn("ultralytics not installed — skipping evaluation.")
            return True

        mon_cfg = self.cfg.load("monitoring.yaml")
        dataset_cfg = ctx.get("dataset_cfg", self.cfg.dataset)
        train_cfg   = self.cfg.load("training.yaml")

        # ── Resolve weights ────────────────────────────────────────────────
        weights = self._resolve_weights(ctx)
        if weights is None:
            self._warn("No model weights found — skipping evaluation.")
            ctx["eval_skipped"] = True
            return True

        # ── Build absolute data.yaml ───────────────────────────────────────
        import tempfile, yaml as _yaml
        data_yaml_path = (
            PROJECT_ROOT
            / dataset_cfg["dataset_path"]
            / dataset_cfg.get("data_yaml_name", "data.yaml")
        )
        with open(data_yaml_path, "r", encoding="utf-8") as fh:
            data = _yaml.safe_load(fh) or {}
        raw_path = data.get("path", "")
        if raw_path:
            candidate = Path(raw_path)
            resolved = candidate if candidate.is_absolute() else (PROJECT_ROOT / raw_path).resolve()
            data["path"] = str(resolved).replace("\\", "/")
        tmp = tempfile.NamedTemporaryFile(
            suffix=".yaml", delete=False, mode="w", encoding="utf-8", prefix="eval_"
        )
        _yaml.dump(data, tmp, sort_keys=False, allow_unicode=True)
        tmp.close()
        abs_data_yaml = Path(tmp.name)

        # ── Run evaluation ─────────────────────────────────────────────────
        eval_split  = mon_cfg.get("eval_split", "val")
        eval_conf   = float(mon_cfg.get("eval_conf", 0.25))
        eval_iou    = float(mon_cfg.get("eval_iou", 0.50))
        device      = str(train_cfg.get("device", "0"))

        self._log(f"Evaluating {weights.name} on split={eval_split} …")
        try:
            model = YOLO(str(weights))
            results = model.val(
                data=str(abs_data_yaml),
                split=eval_split,
                conf=eval_conf,
                iou=eval_iou,
                device=device,
                verbose=False,
            )
        except Exception as exc:
            self._error(f"YOLO val() failed: {exc}")
            return False

        # ── Extract per-class metrics ──────────────────────────────────────
        _CLASS_NAMES = {0: "fire", 1: "smoke"}
        eval_metrics: dict[str, float] = {}
        try:
            box = results.box
            ap_class_index = box.ap_class_index.tolist() if hasattr(box.ap_class_index, "tolist") else list(box.ap_class_index)
            for i, cls_idx in enumerate(ap_class_index):
                cls_name = _CLASS_NAMES.get(int(cls_idx), f"cls{int(cls_idx)}")
                eval_metrics[f"eval.{cls_name}.mAP50"]     = float(box.ap[i])
                eval_metrics[f"eval.{cls_name}.precision"]  = float(box.p[i])
                eval_metrics[f"eval.{cls_name}.recall"]     = float(box.r[i])
            eval_metrics["eval.mAP50"]    = float(results.box.map50)
            eval_metrics["eval.mAP50_95"] = float(results.box.map)
        except Exception as exc:
            self._warn(f"Could not extract per-class metrics: {exc}")

        ctx["eval_metrics"] = eval_metrics
        self._log(f"Evaluation metrics: {eval_metrics}")

        # ── Log to MLflow (new eval run) ───────────────────────────────────
        try:
            import mlflow
            from src.tracking.mlflow_client import MLflowClient
            client = MLflowClient(config_loader=self.cfg)
            tags = {"phase": "evaluate", "split": eval_split, "weights": str(weights.name)}
            with client.run(run_name=f"evaluate_{eval_split}", tags=tags):
                client.log_params({"weights": str(weights.relative_to(PROJECT_ROOT)), "split": eval_split})
                if eval_metrics:
                    client.log_metrics(eval_metrics)
            self._log("Evaluation metrics logged to MLflow.")
        except Exception as exc:
            self._warn(f"MLflow eval logging failed (non-fatal): {exc}")

        # ── Export reference data for Evidently ───────────────────────────
        ref_records = self._sample_reference_data(
            model, abs_data_yaml, eval_split,
            max_samples=int(mon_cfg.get("eval_reference_sample", 300)),
            device=device,
        )
        if ref_records:
            import json
            ref_path = PROJECT_ROOT / "reports" / "evaluation" / "evidently_reference.json"
            ref_path.write_text(json.dumps(ref_records, indent=2), encoding="utf-8")
            ctx["eval_reference_data"] = ref_records
            ctx["eval_reference_path"] = ref_path
            self._log(f"Reference data exported: {len(ref_records)} records → {ref_path}")

        return True

    # ------------------------------------------------------------------

    def _resolve_weights(self, ctx: dict[str, Any]) -> Path | None:
        """Return best.pt: from last train run → or config fallback."""
        train_run = ctx.get("train_run_name")
        train_cfg = self.cfg.load("training.yaml")
        if train_run:
            candidate = PROJECT_ROOT / train_cfg.get("project", "runs/train") / train_run / "weights" / "best.pt"
            if candidate.exists():
                return candidate
        # Fallback: scan for most recent best.pt
        project_dir = PROJECT_ROOT / train_cfg.get("project", "runs/train")
        candidates = sorted(
            project_dir.glob("*/weights/best.pt"),
            key=lambda p: p.stat().st_mtime,
            reverse=True,
        )
        if candidates:
            return candidates[0]
        # Final fallback: init weights from config
        model_cfg = self.cfg.model
        init_w = PROJECT_ROOT / model_cfg["init_weights_local_path"]
        if init_w.exists():
            return init_w
        return None

    def _sample_reference_data(
        self,
        model,
        data_yaml: Path,
        split: str,
        max_samples: int,
        device: str,
    ) -> list[dict]:
        """Run inference on a random sample of the split images, return per-detection records."""
        import random
        import yaml as _yaml
        import numpy as np

        try:
            with open(data_yaml, "r", encoding="utf-8") as fh:
                data = _yaml.safe_load(fh) or {}
            dataset_root = Path(data.get("path", ""))
            images_dir = dataset_root / split / "images"
            if not images_dir.exists():
                images_dir = dataset_root / data.get(split, split) / "images"
            if not images_dir.exists():
                self._warn(f"Images dir not found for split={split}: {images_dir}")
                return []

            image_files = list(images_dir.glob("*.jpg")) + list(images_dir.glob("*.png"))
            if not image_files:
                return []

            sample = random.sample(image_files, min(max_samples, len(image_files)))
            records: list[dict] = []
            _CLASS_NAMES = {0: "fire", 1: "smoke"}

            for img_path in sample:
                import cv2
                frame = cv2.imread(str(img_path))
                if frame is None:
                    continue
                preds = model.predict(frame, conf=0.1, iou=0.45, verbose=False)
                detections = []
                for r in preds:
                    for box in r.boxes:
                        detections.append({
                            "class_name": _CLASS_NAMES.get(int(box.cls[0]), "unknown"),
                            "confidence":  float(box.conf[0]),
                        })

                # Build one record per frame (aggregate stats)
                confs_fire  = [d["confidence"] for d in detections if d["class_name"] == "fire"]
                confs_smoke = [d["confidence"] for d in detections if d["class_name"] == "smoke"]
                record = {
                    "detections_per_frame": float(len(detections)),
                    "confidence_fire":  float(np.mean(confs_fire))  if confs_fire  else 0.0,
                    "confidence_smoke": float(np.mean(confs_smoke)) if confs_smoke else 0.0,
                    "fire_ratio": float(len(confs_fire) / max(len(detections), 1)),
                    "prediction": "fire" if confs_fire else ("smoke" if confs_smoke else "none"),
                }
                records.append(record)

        except Exception as exc:
            self._warn(f"Reference data sampling failed: {exc}")
            return []

        return records


# ---------------------------------------------------------------------------
# Stage 11 – ModelRegisterStage
# ---------------------------------------------------------------------------

class ModelRegisterStage(BaseStage):
    name = "ModelRegister"

    def run(self, ctx: dict[str, Any]) -> bool:
        """Register best.pt in the MLflow Model Registry as 'fire-detection-yolo'."""
        try:
            import mlflow
            from src.tracking.mlflow_client import MLflowClient
        except ImportError:
            self._warn("mlflow not installed — skipping model registration.")
            return True

        # Locate best.pt
        best_pt = self._find_best_pt(ctx)
        if best_pt is None:
            self._warn("No best.pt found — skipping model registration.")
            ctx["register_skipped"] = True
            return True

        self._log(f"Registering model: {best_pt}")

        try:
            client_raw = MLflowClient(config_loader=self.cfg)
            client_raw.configure()
            mlflow_client = mlflow.tracking.MlflowClient()

            eval_metrics = ctx.get("eval_metrics", {})
            map50 = eval_metrics.get("eval.mAP50", 0.0)

            tags = {
                "phase": "register",
                "weights": str(best_pt.relative_to(PROJECT_ROOT)),
                "mAP50": str(round(map50, 4)),
                "dataset": self.cfg.dataset.get("dataset_name", "unknown"),
            }

            # Log the model artifact in a dedicated register run
            with client_raw.run(run_name="model-register", tags=tags) as active_run:
                run_id = active_run.info.run_id
                mlflow.log_artifact(str(best_pt), artifact_path="weights")
                mlflow.log_params({"mAP50": map50, "weights_file": best_pt.name})

            # Register the logged artifact as a model version
            model_name = "fire-detection-yolo"
            model_uri  = f"runs:/{run_id}/weights/{best_pt.name}"
            mv = mlflow.register_model(model_uri=model_uri, name=model_name)

            # Set alias to "production" (MLflow >= 2.x)
            try:
                mlflow_client.set_registered_model_alias(
                    name=model_name, alias="production", version=mv.version
                )
                self._log(f"Model '{model_name}' v{mv.version} → alias=production")
            except Exception:
                # Fallback for older MLflow: use stage transition
                mlflow_client.transition_model_version_stage(
                    name=model_name, version=mv.version, stage="Production"
                )
                self._log(f"Model '{model_name}' v{mv.version} → stage=Production")

            ctx["registered_model_name"]    = model_name
            ctx["registered_model_version"] = mv.version
            return True

        except Exception as exc:
            self._error(f"Model registration failed: {exc}")
            self._error("Is MLflow running?  cd infra/mlflow && docker compose up -d")
            ctx["register_error"] = str(exc)
            return False   # Non-fatal — registration failure should not abort pipeline

    def _find_best_pt(self, ctx: dict[str, Any]) -> Path | None:
        train_cfg  = self.cfg.load("training.yaml")
        project_dir = PROJECT_ROOT / train_cfg.get("project", "runs/train")

        # 1. From current train run in ctx
        train_run = ctx.get("train_run_name")
        if train_run:
            p = project_dir / train_run / "weights" / "best.pt"
            if p.exists():
                return p

        # 2. Most recent run
        candidates = sorted(
            project_dir.glob("*/weights/best.pt"),
            key=lambda p: p.stat().st_mtime,
            reverse=True,
        )
        if candidates:
            return candidates[0]

        # 3. Init weights fallback
        init_w = PROJECT_ROOT / self.cfg.model["init_weights_local_path"]
        return init_w if init_w.exists() else None


# ---------------------------------------------------------------------------
# Stage 12 – MonitoringCheckStage
# ---------------------------------------------------------------------------

class MonitoringCheckStage(BaseStage):
    name = "MonitoringCheck"

    def run(self, ctx: dict[str, Any]) -> bool:
        """Verify monitoring stack health and upload reference data to Evidently."""
        mon_cfg = self.cfg.load("monitoring.yaml")

        results = {
            "prometheus": self._check(mon_cfg.get("prometheus_url", "http://localhost:9090") + "/-/healthy"),
            "grafana":    self._check(mon_cfg.get("grafana_url",    "http://localhost:3000") + "/api/health"),
            "evidently":  self._check(mon_cfg.get("evidently_url",  "http://localhost:8001") + "/health"),
        }

        for svc, ok in results.items():
            if ok:
                self._log(f"{svc}: healthy")
            else:
                self._warn(f"{svc}: unreachable (start infra/monitoring stack)")

        ctx["monitoring_health"] = results

        # ── Upload reference data to Evidently ────────────────────────────
        ref_records = ctx.get("eval_reference_data")
        if ref_records and results.get("evidently"):
            from src.monitoring.evidently_client import EvidentlyClient
            ev = EvidentlyClient(base_url=mon_cfg.get("evidently_url", "http://localhost:8001"))
            ok = ev.upload_reference(
                ref_records,
                description=f"Fire detection test-set — {len(ref_records)} frames",
            )
            if ok:
                self._log(f"Reference data uploaded to Evidently ({len(ref_records)} records).")
            else:
                self._warn("Reference data upload to Evidently failed (non-fatal).")
        elif ref_records and not results.get("evidently"):
            self._warn("Evidently not running — reference data not uploaded. Start monitoring stack first.")

        return True   # Always non-fatal — monitoring is optional

    @staticmethod
    def _check(url: str) -> bool:
        try:
            import urllib.request
            req = urllib.request.Request(url, method="GET")
            with urllib.request.urlopen(req, timeout=4) as r:
                return r.status < 400
        except Exception:
            return False
