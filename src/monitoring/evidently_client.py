"""
HTTP client for pushing fire detection inference data to the Evidently drift service.

All calls are best-effort: failures are logged at DEBUG level and never raise.
This ensures the monitoring layer never impacts the inference pipeline.
"""
from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)

_DEFAULT_TIMEOUT = 3.0   # seconds — low to not block inference thread


class EvidentlyClient:
    """Thin wrapper around httpx for Evidently REST API calls.

    Designed to be used from the pipeline (bulk reference upload) and
    optionally from the API (per-frame capture with sampling).
    """

    def __init__(self, base_url: str = "http://localhost:8001", timeout: float = _DEFAULT_TIMEOUT) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    # ------------------------------------------------------------------
    # Health
    # ------------------------------------------------------------------

    def is_healthy(self) -> bool:
        """Return True if the Evidently service answers /health with 200."""
        try:
            import httpx
            r = httpx.get(f"{self.base_url}/health", timeout=self.timeout)
            return r.status_code == 200
        except Exception as exc:
            logger.debug(f"[Evidently] Health check failed: {exc}")
            return False

    # ------------------------------------------------------------------
    # Data capture (production stream)
    # ------------------------------------------------------------------

    def capture(
        self,
        features: dict[str, float],
        prediction: str | None = None,
    ) -> bool:
        """Capture a single inference record for drift monitoring.

        features: dict with keys like confidence_fire, confidence_smoke,
                  detections_per_frame, fire_ratio …
        """
        try:
            import httpx
            payload = {
                "features": features,
                "prediction": prediction,
            }
            r = httpx.post(f"{self.base_url}/capture", json=payload, timeout=self.timeout)
            return r.status_code == 200
        except Exception as exc:
            logger.debug(f"[Evidently] Capture failed: {exc}")
            return False

    def capture_batch(self, records: list[dict[str, Any]]) -> bool:
        """Capture a batch of inference records."""
        if not records:
            return True
        try:
            import httpx
            r = httpx.post(
                f"{self.base_url}/capture/batch",
                json={"data": records},
                timeout=max(self.timeout, 10.0),
            )
            return r.status_code == 200
        except Exception as exc:
            logger.debug(f"[Evidently] Batch capture failed: {exc}")
            return False

    # ------------------------------------------------------------------
    # Reference data (uploaded once from pipeline after evaluation)
    # ------------------------------------------------------------------

    def upload_reference(
        self,
        records: list[dict[str, Any]],
        description: str = "Fire detection test-set reference",
    ) -> bool:
        """Upload reference data extracted from test-set evaluation.

        records: list of dicts with the same feature keys used in capture().
        """
        if not records:
            logger.warning("[Evidently] No reference records to upload")
            return False
        feature_names = [k for k in records[0].keys() if k != "prediction"]
        try:
            import httpx
            payload = {
                "data": records,
                "feature_names": feature_names,
                "description": description,
            }
            r = httpx.post(
                f"{self.base_url}/reference",
                json=payload,
                timeout=60.0,   # large upload may take time
            )
            ok = r.status_code == 200
            if ok:
                logger.info(f"[Evidently] Reference data uploaded: {len(records)} records")
            else:
                logger.warning(f"[Evidently] Reference upload failed: {r.status_code} {r.text[:200]}")
            return ok
        except Exception as exc:
            logger.warning(f"[Evidently] Reference upload error: {exc}")
            return False

    # ------------------------------------------------------------------
    # Trigger analysis
    # ------------------------------------------------------------------

    def trigger_analysis(self, window_size: int = 200, threshold: float = 0.1) -> dict | None:
        """Trigger a drift analysis run on the Evidently service."""
        try:
            import httpx
            payload = {"window_size": window_size, "threshold": threshold}
            r = httpx.post(f"{self.base_url}/analyze", json=payload, timeout=60.0)
            if r.status_code == 200:
                return r.json()
            return None
        except Exception as exc:
            logger.debug(f"[Evidently] Trigger analysis failed: {exc}")
            return None
