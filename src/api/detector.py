"""
FireDetector — wraps Ultralytics YOLO for single-frame inference.
Loaded once at startup, reused across all camera streams.
"""
from __future__ import annotations

import threading
import time
from pathlib import Path
from typing import Any

import numpy as np

from src.core.config_loader import PROJECT_ROOT
from src.core.logger import get_logger

logger = get_logger()

# class_id → label (matches D-Fire remapped labels: fire=0, smoke=1)
_CLASS_NAMES = {0: "fire", 1: "smoke"}

# Prometheus metrics — imported lazily to allow the module to load even if
# prometheus_client is not installed (unit tests, lightweight environments).
def _metrics():
    try:
        from src.monitoring import metrics as m
        return m
    except ImportError:
        return None


def _resolve_model(cfg: dict) -> Path:
    """Return first existing model path from config."""
    for key in ("model_path", "model_fallback_path"):
        p = PROJECT_ROOT / cfg.get(key, "")
        if p.exists():
            logger.info(f"[Detector] Using model: {p}")
            return p
    raise FileNotFoundError(
        "No model found. Check model_path / model_fallback_path in config/api.yaml"
    )


class FireDetector:
    """Thread-safe YOLO inference wrapper."""

    def __init__(self, config: dict) -> None:
        from ultralytics import YOLO

        model_path = _resolve_model(config)
        self.model = YOLO(str(model_path))
        self.conf: float = float(config.get("confidence_threshold", 0.25))
        self.iou: float = float(config.get("iou_threshold", 0.45))
        self._lock = threading.Lock()   # YOLO model is not thread-safe for concurrent inference
        logger.info(f"[Detector] Ready — conf={self.conf} iou={self.iou}")

    def predict(self, frame_bgr: np.ndarray, camera_id: str = "unknown") -> list[dict[str, Any]]:
        """Run inference on a single BGR frame (from OpenCV).

        Returns list of dicts:
            {"class": str, "confidence": float, "bbox": [x1, y1, x2, y2]}
        """
        m = _metrics()
        t0 = time.perf_counter()

        try:
            with self._lock:
                results = self.model.predict(
                    frame_bgr,
                    conf=self.conf,
                    iou=self.iou,
                    verbose=False,
                    stream=False,
                )
        except Exception:
            if m:
                m.INFERENCE_ERRORS.labels(error_type="predict_error").inc()
            raise

        elapsed = time.perf_counter() - t0
        if m:
            m.INFERENCE_LATENCY.observe(elapsed)
            m.FRAMES_PROCESSED.labels(camera_id=camera_id).inc()

        detections: list[dict[str, Any]] = []
        for result in results:
            for box in result.boxes:
                cls_id = int(box.cls[0])
                conf = float(box.conf[0])
                x1, y1, x2, y2 = (int(v) for v in box.xyxy[0].tolist())
                class_name = _CLASS_NAMES.get(cls_id, f"cls{cls_id}")
                detections.append({
                    "class": class_name,
                    "confidence": round(conf, 3),
                    "bbox": [x1, y1, x2, y2],
                })
                if m:
                    m.DETECTIONS_TOTAL.labels(class_name=class_name).inc()
                    m.DETECTION_CONFIDENCE.labels(class_name=class_name).observe(conf)

        if m:
            m.DETECTIONS_PER_FRAME.observe(len(detections))

        return detections
