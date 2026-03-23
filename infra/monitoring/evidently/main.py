"""
============================================================
Evidently AI — Fire Detection Drift Service
============================================================

Adapted from ml-monitoring for fire/smoke YOLO inference.

Features tracked per-frame:
  confidence_fire       — mean confidence of fire detections  (0 if none)
  confidence_smoke      — mean confidence of smoke detections (0 if none)
  detections_per_frame  — total objects detected in the frame
  fire_ratio            — fire detections / total detections  (0 if none)

Reference data is uploaded once by the MonitoringCheckStage pipeline stage
after ModelEvaluateStage extracts predictions from the test split.

Endpoints:
  GET  /health            — service health
  POST /capture           — capture one inference record
  POST /capture/batch     — capture many records
  POST /reference         — upload reference data (from pipeline)
  GET  /reference         — reference data info
  POST /analyze           — run drift analysis → Prometheus metrics + HTML report
  GET  /reports           — list HTML reports
  GET  /reports/{name}    — view specific report
  GET  /metrics           — Prometheus metrics scrape endpoint
============================================================
"""
from __future__ import annotations

import json
import logging
import os
import time
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional

import numpy as np
import pandas as pd
from evidently.metrics import ColumnDriftMetric, DatasetDriftMetric
from evidently.report import Report
from fastapi import BackgroundTasks, FastAPI, HTTPException
from fastapi.responses import HTMLResponse
from prometheus_client import (
    CONTENT_TYPE_LATEST,
    Counter,
    Gauge,
    Histogram,
    generate_latest,
)
from pydantic import BaseModel, Field
from starlette.responses import Response

# ── Configuration ─────────────────────────────────────────────────────────────

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s  %(name)s  %(levelname)s  %(message)s",
)
logger = logging.getLogger("fire-evidently")

REPORTS_DIR   = Path("/app/reports")
DATA_DIR      = Path("/app/data")
REFERENCE_DIR = Path("/app/reference")

for _d in (REPORTS_DIR, DATA_DIR, REFERENCE_DIR):
    _d.mkdir(parents=True, exist_ok=True)

DRIFT_THRESHOLD  = float(os.getenv("EVIDENTLY_DRIFT_THRESHOLD", "0.1"))
MIN_SAMPLES      = int(os.getenv("EVIDENTLY_MIN_SAMPLES", "50"))

# ── Prometheus Metrics ────────────────────────────────────────────────────────

DRIFT_DETECTED = Gauge(
    "evidently_data_drift_detected",
    "Whether data drift is detected (1=yes, 0=no)",
)
DRIFT_SCORE = Gauge(
    "evidently_drift_score",
    "Overall drift share (fraction of drifted columns)",
)
FEATURE_DRIFT = Gauge(
    "evidently_feature_drift",
    "Drift detected per feature (1=drift, 0=ok)",
    ["feature_name"],
)
DRIFTED_FEATURES_COUNT = Gauge(
    "evidently_drifted_features_count",
    "Number of features with detected drift",
)
ANALYSIS_COUNT = Counter(
    "evidently_analysis_total",
    "Total number of drift analyses performed",
)
ANALYSIS_DURATION = Histogram(
    "evidently_analysis_duration_seconds",
    "Time taken to perform a drift analysis run",
    buckets=[0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0],
)
MISSING_VALUES = Gauge(
    "evidently_missing_values_ratio",
    "Ratio of missing values per feature",
    ["feature_name"],
)
PRODUCTION_DATA_SIZE = Gauge(
    "evidently_production_data_size",
    "Number of records in the current production data buffer",
)

# ── Pydantic Models ───────────────────────────────────────────────────────────

class PredictionData(BaseModel):
    """One inference record captured from the live stream."""
    features: Dict[str, float]          # confidence_fire, confidence_smoke, …
    prediction: Optional[str] = None    # "fire" | "smoke" | "none"
    timestamp: Optional[str] = None

class BatchPredictionData(BaseModel):
    data: List[Dict[str, Any]]
    feature_names: Optional[List[str]] = None

class DriftAnalysisRequest(BaseModel):
    window_size: Optional[int]   = Field(200, description="Recent production samples to analyse")
    threshold:   Optional[float] = Field(None, description="Override default drift threshold")

class ReferenceDataRequest(BaseModel):
    data:          List[Dict[str, Any]]
    feature_names: List[str]
    description:   Optional[str] = None

class HealthResponse(BaseModel):
    status:                  str
    reference_data_loaded:   bool
    production_data_count:   int
    last_analysis:           Optional[str]
    reports_count:           int
    drift_threshold:         float
    min_samples:             int

# ── Data Store ────────────────────────────────────────────────────────────────

# Fire-detection feature columns tracked for drift
FEATURE_COLS = [
    "confidence_fire",
    "confidence_smoke",
    "detections_per_frame",
    "fire_ratio",
]

class DataStore:
    def __init__(self):
        self.reference_data: Optional[pd.DataFrame] = None
        self.production_data: List[Dict] = []
        self.last_analysis_time: Optional[datetime] = None
        self.reference_metadata: Dict = {}
        self._load_reference()

    def _load_reference(self):
        ref_file  = REFERENCE_DIR / "reference_data.csv"
        meta_file = REFERENCE_DIR / "metadata.json"
        if ref_file.exists():
            try:
                self.reference_data = pd.read_csv(ref_file)
                logger.info(f"Reference data loaded: {len(self.reference_data)} rows, cols={list(self.reference_data.columns)}")
            except Exception as exc:
                logger.error(f"Failed to load reference data: {exc}")
        if meta_file.exists():
            try:
                self.reference_metadata = json.loads(meta_file.read_text())
            except Exception:
                pass

    def save_reference(self, df: pd.DataFrame, metadata: Dict):
        try:
            df.to_csv(REFERENCE_DIR / "reference_data.csv", index=False)
            (REFERENCE_DIR / "metadata.json").write_text(json.dumps(metadata, indent=2))
            self.reference_data = df
            self.reference_metadata = metadata
            logger.info(f"Reference data saved: {len(df)} rows")
        except Exception as exc:
            logger.error(f"Failed to save reference data: {exc}")
            raise

    def add_production(self, record: Dict):
        self.production_data.append(record)
        if len(self.production_data) > 10_000:
            self.production_data = self.production_data[-10_000:]
        PRODUCTION_DATA_SIZE.set(len(self.production_data))

    def get_production_df(self, window: Optional[int] = None) -> pd.DataFrame:
        data = self.production_data[-window:] if window else self.production_data
        return pd.DataFrame(data) if data else pd.DataFrame()

    def clear_production(self):
        self.production_data = []
        PRODUCTION_DATA_SIZE.set(0)


data_store = DataStore()
app_start  = datetime.now()
app = FastAPI(title="Fire Detection — Evidently Drift Service", version="1.0.0")

# ── Helpers ───────────────────────────────────────────────────────────────────

def _align_features(df: pd.DataFrame, target_cols: List[str]) -> pd.DataFrame:
    """Keep only numeric feature columns; fill missing with 0."""
    for col in target_cols:
        if col not in df.columns:
            df[col] = 0.0
    numeric_cols = [c for c in target_cols if c in df.columns]
    return df[numeric_cols].fillna(0.0)

# ── Endpoints ─────────────────────────────────────────────────────────────────

@app.get("/")
async def root():
    return {
        "service":  "Fire Detection — Evidently Drift Service",
        "version":  "1.0.0",
        "features": FEATURE_COLS,
        "endpoints": {
            "health":        "GET  /health",
            "metrics":       "GET  /metrics",
            "capture":       "POST /capture",
            "capture_batch": "POST /capture/batch",
            "reference":     "GET|POST /reference",
            "analyze":       "POST /analyze",
            "reports":       "GET  /reports",
        },
    }


@app.get("/health", response_model=HealthResponse)
async def health():
    reports = list(REPORTS_DIR.glob("*.html"))
    return HealthResponse(
        status="healthy",
        reference_data_loaded=data_store.reference_data is not None,
        production_data_count=len(data_store.production_data),
        last_analysis=data_store.last_analysis_time.isoformat() if data_store.last_analysis_time else None,
        reports_count=len(reports),
        drift_threshold=DRIFT_THRESHOLD,
        min_samples=MIN_SAMPLES,
    )


@app.post("/capture")
async def capture(data: PredictionData):
    """Capture one inference record (called per-frame from the API with sampling)."""
    record = {
        **{k: float(v) for k, v in data.features.items()},
        "prediction": data.prediction,
        "timestamp":  data.timestamp or datetime.now().isoformat(),
    }
    data_store.add_production(record)
    return {"status": "ok", "total_samples": len(data_store.production_data)}


@app.post("/capture/batch")
async def capture_batch(data: BatchPredictionData):
    """Capture multiple inference records at once."""
    for item in data.data:
        data_store.add_production(item)
    return {"status": "ok", "captured": len(data.data), "total_samples": len(data_store.production_data)}


@app.get("/reference")
async def get_reference():
    if data_store.reference_data is None:
        return {"loaded": False, "message": "No reference data loaded"}
    return {
        "loaded":   True,
        "samples":  len(data_store.reference_data),
        "features": list(data_store.reference_data.columns),
        "metadata": data_store.reference_metadata,
    }


@app.post("/reference")
async def upload_reference(req: ReferenceDataRequest):
    """Upload reference data from the pipeline's ModelEvaluateStage."""
    if not req.data:
        raise HTTPException(status_code=400, detail="Empty dataset")

    df = pd.DataFrame(req.data)
    # Keep only numeric feature columns
    df = df[[c for c in FEATURE_COLS if c in df.columns]].fillna(0.0)
    if df.empty:
        raise HTTPException(status_code=400, detail=f"None of the expected features found. Expected: {FEATURE_COLS}")

    metadata = {
        "description": req.description or "Fire detection reference dataset",
        "uploaded_at": datetime.now().isoformat(),
        "samples":     len(df),
        "features":    list(df.columns),
    }
    data_store.save_reference(df, metadata)
    return {"status": "ok", "samples": len(df), "features": list(df.columns)}


@app.post("/analyze")
async def analyze_drift(req: DriftAnalysisRequest = DriftAnalysisRequest()):
    """Run Evidently drift analysis on recent production data vs reference data."""
    if data_store.reference_data is None:
        raise HTTPException(status_code=400, detail="No reference data. Upload reference via POST /reference first.")

    prod_df = data_store.get_production_df(req.window_size)
    if len(prod_df) < MIN_SAMPLES:
        raise HTTPException(
            status_code=400,
            detail=f"Not enough production data: {len(prod_df)} < {MIN_SAMPLES} (minimum). Keep capturing data."
        )

    threshold = req.threshold if req.threshold is not None else DRIFT_THRESHOLD
    logger.info(f"Drift analysis: ref={len(data_store.reference_data)} prod={len(prod_df)} threshold={threshold}")

    t0 = time.perf_counter()
    result = _run_drift_analysis(data_store.reference_data, prod_df, threshold)
    duration = time.perf_counter() - t0

    ANALYSIS_COUNT.inc()
    ANALYSIS_DURATION.observe(duration)
    data_store.last_analysis_time = datetime.now()

    logger.info(f"Analysis done in {duration:.2f}s — drift={result['drift_detected']} score={result['drift_score']:.3f}")
    return result


@app.get("/reports")
async def list_reports():
    reports = sorted(REPORTS_DIR.glob("*.html"), key=lambda p: p.stat().st_mtime, reverse=True)
    return {
        "count": len(reports),
        "reports": [
            {
                "filename": r.name,
                "created":  datetime.fromtimestamp(r.stat().st_mtime).isoformat(),
                "size_kb":  round(r.stat().st_size / 1024, 1),
                "url":      f"/reports/{r.name}",
            }
            for r in reports
        ],
    }


@app.get("/reports/{report_name}", response_class=HTMLResponse)
async def get_report(report_name: str):
    path = REPORTS_DIR / report_name
    if not path.exists():
        raise HTTPException(status_code=404, detail="Report not found")
    return path.read_text()


@app.delete("/production-data")
async def clear_production():
    data_store.clear_production()
    return {"status": "ok", "message": "Production data cleared"}


@app.get("/metrics")
async def metrics():
    return Response(content=generate_latest(), media_type=CONTENT_TYPE_LATEST)


# ── Drift Analysis Logic ──────────────────────────────────────────────────────

def _run_drift_analysis(
    reference: pd.DataFrame,
    current: pd.DataFrame,
    threshold: float,
) -> Dict[str, Any]:

    # Align feature columns
    feature_cols = [c for c in FEATURE_COLS if c in reference.columns and c in current.columns]
    if not feature_cols:
        raise ValueError(f"No common feature columns. Expected: {FEATURE_COLS}")

    ref_df  = reference[feature_cols].fillna(0.0)
    curr_df = _align_features(current.copy(), feature_cols)[feature_cols]

    logger.info(f"Analysing {len(feature_cols)} features: {feature_cols}")

    # Build report: DatasetDriftMetric + per-column ColumnDriftMetric
    metrics_list = [DatasetDriftMetric()] + [ColumnDriftMetric(col) for col in feature_cols]
    report = Report(metrics=metrics_list)
    report.run(reference_data=ref_df, current_data=curr_df)
    report_dict = report.as_dict()

    # Parse results
    drift_detected = False
    drift_score    = 0.0
    drifted_features: List[str] = []
    drift_scores: Dict[str, float] = {}

    for metric in report_dict.get("metrics", []):
        metric_name = metric.get("metric")
        result = metric.get("result", {})

        if metric_name == "DatasetDriftMetric":
            drift_detected = bool(result.get("dataset_drift", False))
            drift_score    = float(result.get("share_of_drifted_columns", 0.0))

        elif metric_name == "ColumnDriftMetric":
            feature    = result.get("column_name", "unknown")
            is_drifted = bool(result.get("drift_detected", False))
            score      = float(result.get("drift_score", 0.0))
            drift_scores[feature] = score
            if is_drifted:
                drifted_features.append(feature)
            FEATURE_DRIFT.labels(feature_name=feature).set(1 if is_drifted else 0)

            # Missing values from column stats
            missing_count = result.get("current", {}).get("number_of_missing", 0)
            total         = result.get("current", {}).get("number_of_rows", 1) or 1
            MISSING_VALUES.labels(feature_name=feature).set(missing_count / total)

    # Update Prometheus
    DRIFT_DETECTED.set(1 if drift_detected else 0)
    DRIFT_SCORE.set(drift_score)
    DRIFTED_FEATURES_COUNT.set(len(drifted_features))

    # Save HTML report
    ts          = datetime.now().strftime("%Y%m%d_%H%M%S")
    report_file = f"drift_report_{ts}.html"
    report.save_html(str(REPORTS_DIR / report_file))
    logger.info(f"Drift report saved: {report_file}")

    return {
        "status":           "ok",
        "timestamp":        datetime.now().isoformat(),
        "drift_detected":   drift_detected,
        "drift_score":      drift_score,
        "drifted_features": drifted_features,
        "drift_scores":     drift_scores,
        "total_features":   len(feature_cols),
        "drifted_count":    len(drifted_features),
        "report_url":       f"/reports/{report_file}",
        "reference_samples": len(ref_df),
        "current_samples":   len(curr_df),
    }


# ── Startup ───────────────────────────────────────────────────────────────────

@app.on_event("startup")
async def startup():
    logger.info("=" * 60)
    logger.info("Fire Detection — Evidently Drift Service starting")
    logger.info(f"  Features tracked : {FEATURE_COLS}")
    logger.info(f"  Drift threshold  : {DRIFT_THRESHOLD}")
    logger.info(f"  Min samples      : {MIN_SAMPLES}")
    if data_store.reference_data is not None:
        logger.info(f"  Reference data   : {len(data_store.reference_data)} rows (loaded from disk)")
    else:
        logger.warning("  Reference data   : NOT LOADED — upload via POST /reference or run pipeline MonitoringCheckStage")
    logger.info("=" * 60)


if __name__ == "__main__":
    import uvicorn
    uvicorn.run("main:app", host="0.0.0.0", port=8001, reload=False)
