"""
Pipeline orchestrator: runs all framework stages in order.

Usage (from project root):
    python -m src.pipeline.orchestrator
"""
from __future__ import annotations

import sys
import time
from typing import Any

from src.core.config_loader import ConfigLoader
from src.core.logger import get_logger
from src.pipeline.stages import (
    BaseStage,
    DatasetAnalyzeStage,
    DatasetReportExportStage,
    DatasetValidateStage,
    EnvFingerprintStage,
    ExperimentSummaryExportStage,
    InitWeightsManifestStage,
    LoadConfigStage,
    MLflowSmokeRunStage,
    ModelEvaluateStage,
    ModelRegisterStage,
    MonitoringCheckStage,
    TrainStage,
)

logger = get_logger()


BANNER = """
==========================================================
  Fire Detection Pipeline
  Phase: TRAIN + EVALUATE + MONITOR
  (fine-tune YOLOv10x → evaluate → register → check monitoring)
==========================================================
"""


class PipelineOrchestrator:
    """Runs stages sequentially, tracks success/failure, and prints a summary."""

    def __init__(self, config_loader: ConfigLoader | None = None) -> None:
        self.cfg = config_loader or ConfigLoader()
        self.stages: list[BaseStage] = self._build_stages()
        self.ctx: dict[str, Any] = {}

    def _build_stages(self) -> list[BaseStage]:
        stages = [
            LoadConfigStage(self.cfg),           # 1
            EnvFingerprintStage(self.cfg),        # 2
            InitWeightsManifestStage(self.cfg),   # 3
            DatasetValidateStage(self.cfg),       # 4
            DatasetAnalyzeStage(self.cfg),        # 5
            DatasetReportExportStage(self.cfg),   # 6
            MLflowSmokeRunStage(self.cfg),        # 7
            TrainStage(self.cfg),                 # 8
            ModelEvaluateStage(self.cfg),         # 9  — per-class mAP + Evidently reference
            ExperimentSummaryExportStage(self.cfg), # 10 — after train+eval so summary is complete
            ModelRegisterStage(self.cfg),         # 11 — MLflow Model Registry
            MonitoringCheckStage(self.cfg),       # 12 — Prometheus/Grafana/Evidently health
        ]
        return stages

    def run(self) -> bool:
        print(BANNER)
        logger.info("Pipeline starting ...")
        overall_start = time.time()

        results: list[tuple[str, bool, float]] = []

        for stage in self.stages:
            logger.info("-" * 60)
            logger.info(f"Stage: {stage.name}")
            t0 = time.time()
            try:
                ok = stage.run(self.ctx)
            except Exception as exc:
                logger.error(f"[{stage.name}] FATAL: {exc}", exc_info=True)
                ok = False
                results.append((stage.name, False, time.time() - t0))
                self._print_summary(results, overall_start, aborted=True)
                return False
            elapsed = time.time() - t0
            results.append((stage.name, ok, elapsed))
            status_str = "OK" if ok else "WARN"
            logger.info(f"[{stage.name}] {status_str} ({elapsed:.2f}s)")

        self._print_summary(results, overall_start, aborted=False)
        all_ok = all(ok for _, ok, _ in results)
        return all_ok

    def _print_summary(
        self,
        results: list[tuple[str, bool, float]],
        start: float,
        aborted: bool,
    ) -> None:
        total = time.time() - start
        logger.info("=" * 60)
        logger.info("PIPELINE SUMMARY")
        logger.info("-" * 60)
        for name, ok, elapsed in results:
            mark = "[OK]  " if ok else "[FAIL]"
            logger.info(f"  {mark}  {name:<35} {elapsed:>6.2f}s")
        logger.info("-" * 60)
        logger.info(f"  Total elapsed: {total:.2f}s")
        if aborted:
            logger.error("  RESULT: ABORTED (fatal error)")
        else:
            passed = sum(1 for _, ok, _ in results if ok)
            logger.info(f"  RESULT: {passed}/{len(results)} stages OK")
        logger.info("=" * 60)

        # Highlight key outputs
        rpt = self.ctx.get("dataset_report_md")
        stats = self.ctx.get("dataset_stats_json")
        fp = self.ctx.get("env_fingerprint_path")
        mf = self.ctx.get("init_weights_manifest_path")
        exp = self.ctx.get("experiment_summary_path")
        smoke_err = self.ctx.get("mlflow_smoke_error")

        logger.info("Key outputs:")
        for label, path in [
            ("Dataset report  ", rpt),
            ("Dataset stats   ", stats),
            ("Env fingerprint ", fp),
            ("Weights manifest", mf),
            ("Exp summary     ", exp),
        ]:
            if path:
                logger.info(f"  {label}: {path}")

        ref_path  = self.ctx.get("eval_reference_path")
        reg_model = self.ctx.get("registered_model_name")
        reg_ver   = self.ctx.get("registered_model_version")
        mon_health = self.ctx.get("monitoring_health", {})

        for label, path in [
            ("Eval reference  ", ref_path),
        ]:
            if path:
                logger.info(f"  {label}: {path}")

        if reg_model:
            logger.info(f"  MLflow Registry : {reg_model} v{reg_ver}")

        if mon_health:
            ok_svcs  = [k for k, v in mon_health.items() if v]
            nok_svcs = [k for k, v in mon_health.items() if not v]
            if ok_svcs:
                logger.info(f"  Monitoring UP   : {', '.join(ok_svcs)}")
            if nok_svcs:
                logger.warning(f"  Monitoring DOWN : {', '.join(nok_svcs)}  (run: cd infra/monitoring && docker compose up -d)")

        if smoke_err:
            logger.warning(
                "MLflow smoke run SKIPPED (server not available).\n"
                "  Start it with:  cd infra/mlflow && docker compose up -d\n"
                "  Then re-run the orchestrator."
            )
        logger.info("")


def main() -> None:
    # Force UTF-8 on Windows (avoids cp1252 errors from MLflow emoji output)
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    orchestrator = PipelineOrchestrator()
    success = orchestrator.run()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
