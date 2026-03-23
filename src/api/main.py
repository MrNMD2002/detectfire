"""
Fire Detection Inference API — FastAPI server.

Endpoints:
  GET  /                          → Dashboard UI
  GET  /api/cameras               → List active cameras
  POST /api/cameras               → Add camera (RTSP or webcam)
  DELETE /api/cameras/{id}        → Remove camera
  WS   /ws/{camera_id}            → Live stream + detections

Run:
  python -m src.api.main
  # or via uvicorn:
  uvicorn src.api.main:app --host 0.0.0.0 --port 8000 --reload
"""
from __future__ import annotations

import asyncio
import os
import sys
import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from urllib.parse import urlparse

import time

from fastapi import Depends, FastAPI, HTTPException, Request, WebSocket, WebSocketDisconnect
from fastapi.responses import FileResponse
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
from fastapi.staticfiles import StaticFiles
from prometheus_client import CONTENT_TYPE_LATEST, generate_latest
from pydantic import BaseModel, field_validator
from starlette.responses import Response

from src.core.config_loader import ConfigLoader, PROJECT_ROOT
from src.core.logger import get_logger
from src.api.detector import FireDetector
from src.api.stream_manager import StreamManager
from src.monitoring import metrics as _m

logger = get_logger()

_STATIC_DIR = Path(__file__).parent / "static"

# ---------------------------------------------------------------------------
# Global singletons (initialised in lifespan)
# ---------------------------------------------------------------------------
_detector: FireDetector | None = None
_manager:  StreamManager | None = None
_api_key:  str | None = None       # None = authentication disabled


@asynccontextmanager
async def lifespan(app: FastAPI):
    global _detector, _manager, _api_key

    cfg     = ConfigLoader()
    api_cfg = cfg.load("api.yaml")

    # API key: env var takes priority over config file value
    _api_key = os.environ.get("API_KEY") or api_cfg.get("api_key") or None
    if _api_key:
        logger.info("[API] Authentication enabled (Bearer token required for write endpoints)")
    else:
        logger.warning("[API] Authentication disabled — set API_KEY in .env for production")

    logger.info("[API] Loading model…")
    _detector = FireDetector(api_cfg)

    # Expose model info to Prometheus
    model_path = api_cfg.get("model_path", "unknown")
    _m.MODEL_INFO.info({
        "weights_path": str(model_path),
        "conf_threshold": str(api_cfg.get("confidence_threshold", 0.25)),
        "iou_threshold": str(api_cfg.get("iou_threshold", 0.45)),
    })

    _manager = StreamManager(_detector, api_cfg, persist_path=PROJECT_ROOT / "data" / "cameras.json")
    _manager.set_loop(asyncio.get_running_loop())
    _manager.restore_cameras()   # auto-reconnect cameras saved from previous session

    # Sync active camera gauge with restored cameras
    _m.ACTIVE_CAMERAS.set(len(_manager.list_streams()))

    logger.info("[API] Ready — http://localhost:8000")
    yield

    logger.info("[API] Shutting down…")
    if _manager:
        _manager.shutdown()
    _m.ACTIVE_CAMERAS.set(0)


# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------
app = FastAPI(title="Fire Detection API", version="1.0.0", lifespan=lifespan)
app.mount("/static", StaticFiles(directory=str(_STATIC_DIR)), name="static")


# ---------------------------------------------------------------------------
# Middleware — HTTP request tracking
# ---------------------------------------------------------------------------

_SKIP_METRICS_PATH = {"/metrics", "/health"}


@app.middleware("http")
async def track_requests(request: Request, call_next):
    """Record request count and latency for every HTTP endpoint."""
    t0 = time.perf_counter()
    response = await call_next(request)
    elapsed = time.perf_counter() - t0

    path = request.url.path
    if path not in _SKIP_METRICS_PATH:
        _m.API_REQUESTS_TOTAL.labels(
            method=request.method,
            endpoint=path,
            status=str(response.status_code),
        ).inc()
        _m.API_REQUEST_LATENCY.labels(
            method=request.method,
            endpoint=path,
        ).observe(elapsed)

    return response


# ---------------------------------------------------------------------------
# Auth dependency
# ---------------------------------------------------------------------------

_bearer = HTTPBearer(auto_error=False)


def _require_auth(
    credentials: HTTPAuthorizationCredentials | None = Depends(_bearer),
) -> None:
    """Dependency: enforce bearer token on write endpoints when API_KEY is set."""
    if _api_key is None:
        return  # Auth disabled — open access (dev mode)
    if credentials is None or credentials.credentials != _api_key:
        raise HTTPException(status_code=401, detail="Invalid or missing API key")


# ---------------------------------------------------------------------------
# REST — Camera management
# ---------------------------------------------------------------------------

class AddCameraRequest(BaseModel):
    type: str           # "rtsp" | "webcam"
    url:  str | None = None    # RTSP URL
    device_id: int | None = 0  # webcam device index
    name: str | None = None

    @field_validator("type")
    @classmethod
    def validate_type(cls, v: str) -> str:
        if v not in ("rtsp", "webcam"):
            raise ValueError("type must be 'rtsp' or 'webcam'")
        return v

    @field_validator("url")
    @classmethod
    def validate_rtsp_url(cls, v: str | None) -> str | None:
        if v is None:
            return v
        v = v.strip()
        # Catch browser-autocomplete duplicate: "rtsp://...rtsp://..."
        scheme_count = v.lower().count("rtsp://")
        if scheme_count > 1:
            raise ValueError("URL appears to be duplicated — please enter a single RTSP URL")
        parsed = urlparse(v)
        if parsed.scheme not in ("rtsp", "rtsps"):
            raise ValueError("Only rtsp:// or rtsps:// URLs are allowed")
        if not parsed.hostname:
            raise ValueError("URL must contain a valid hostname")
        return v


class ConfigUpdateRequest(BaseModel):
    confidence_threshold: float


class PredictRequest(BaseModel):
    features: list[float]
    feature_names: list[str] = []


@app.post("/predict")
async def predict(req: PredictRequest):
    """Simulation-compatible predict endpoint.

    Accepts a feature vector from the ML-monitoring simulator and returns
    a synthetic fire-detection confidence score.  The response is also
    recorded into Prometheus so the monitoring stack receives live data.
    """
    import numpy as np

    if not req.features:
        raise HTTPException(status_code=400, detail="features must not be empty")

    # Derive a [0, 1] score from the normalised mean of incoming features
    arr = np.array(req.features, dtype=float)
    # Robust normalisation: shift to [0,1] using min/max of the vector
    lo, hi = arr.min(), arr.max()
    normalised = ((arr - lo) / (hi - lo + 1e-9))
    fire_conf  = float(np.clip(normalised.mean(), 0.0, 1.0))
    smoke_conf = float(np.clip(1.0 - fire_conf + np.random.uniform(-0.1, 0.1), 0.0, 1.0))

    # Push into Prometheus (reuse existing fire_* metrics)
    _m.DETECTION_CONFIDENCE.labels(class_name="fire").observe(fire_conf)
    _m.DETECTION_CONFIDENCE.labels(class_name="smoke").observe(smoke_conf)
    if fire_conf > (_detector.conf if _detector else 0.25):
        _m.DETECTIONS_TOTAL.labels(class_name="fire").inc()
    if smoke_conf > (_detector.conf if _detector else 0.25):
        _m.DETECTIONS_TOTAL.labels(class_name="smoke").inc()

    return {
        "prediction": round(fire_conf, 4),
        "fire_confidence": round(fire_conf, 4),
        "smoke_confidence": round(smoke_conf, 4),
        "model_name": "fire_detection",
        "model_version": "1.0.0",
    }


@app.get("/metrics")
async def prometheus_metrics():
    """Prometheus scrape endpoint — exposes all fire_* metrics."""
    return Response(content=generate_latest(), media_type=CONTENT_TYPE_LATEST)


@app.get("/health")
async def health():
    """Health check — used by Docker, load balancers, and Prometheus alerts."""
    return {
        "status": "healthy",
        "model_loaded": _detector is not None,
        "active_cameras": len(_manager.list_streams()) if _manager else 0,
    }


@app.get("/")
async def index():
    return FileResponse(str(_STATIC_DIR / "index.html"))


@app.get("/api/config")
async def get_config():
    return {
        "confidence_threshold": _detector.conf,
        "iou_threshold": _detector.iou,
    }


@app.put("/api/config")
async def update_config(req: ConfigUpdateRequest, _: None = Depends(_require_auth)):
    conf = max(0.05, min(0.99, req.confidence_threshold))
    _detector.conf = conf
    logger.info(f"[API] Confidence threshold updated → {conf:.2f}")
    return {"confidence_threshold": conf}


@app.get("/api/cameras")
async def list_cameras():
    return _manager.list_streams()


@app.get("/api/webcams")
async def list_webcams():
    """Scan device indices 0-9 and return available webcam devices."""
    import cv2
    import asyncio

    def _scan() -> list[dict]:
        found = []
        for i in range(10):
            cap = cv2.VideoCapture(i)
            if cap.isOpened():
                ret, _ = cap.read()
                if ret:
                    found.append({"index": i, "name": f"Webcam {i}"})
                cap.release()
        return found

    # Run blocking scan in thread pool so we don't block the event loop
    loop = asyncio.get_event_loop()
    devices = await loop.run_in_executor(None, _scan)
    return devices


@app.post("/api/cameras", status_code=201)
async def add_camera(req: AddCameraRequest, _: None = Depends(_require_auth)):
    camera_id = str(uuid.uuid4())[:8]
    name      = req.name or (f"RTSP {camera_id}" if req.type == "rtsp" else f"Webcam {req.device_id}")

    if req.type == "rtsp":
        if not req.url:
            raise HTTPException(status_code=400, detail="url required for RTSP camera")
        source = req.url
    else:
        source = int(req.device_id) if req.device_id is not None else 0

    _manager.add_stream(camera_id, source, name)
    _m.ACTIVE_CAMERAS.set(len(_manager.list_streams()))
    return {"id": camera_id, "name": name}


@app.delete("/api/cameras/{camera_id}")
async def remove_camera(camera_id: str, _: None = Depends(_require_auth)):
    removed = _manager.remove_stream(camera_id)
    _m.ACTIVE_CAMERAS.set(len(_manager.list_streams()))
    return {"removed": removed}


# ---------------------------------------------------------------------------
# WebSocket — Live stream
# ---------------------------------------------------------------------------

@app.websocket("/ws/{camera_id}")
async def ws_stream(websocket: WebSocket, camera_id: str):
    await websocket.accept()

    stream = _manager.get_stream(camera_id)
    if stream is None:
        await websocket.send_json({"type": "error", "message": f"Camera {camera_id!r} not found"})
        await websocket.close()
        return

    queue = stream.subscribe()   # broadcast receiver — mirrors stream.subscribe() in MBFS_Stream
    logger.info(f"[WS] Client connected to camera {camera_id}")

    try:
        while True:
            data = await queue.get()
            await websocket.send_json(data)
    except WebSocketDisconnect:
        pass
    except Exception as exc:
        logger.warning(f"[WS] camera={camera_id} error: {exc}")
    finally:
        stream.unsubscribe(queue)   # mirrors rx drop in Rust
        logger.info(f"[WS] Client disconnected from camera {camera_id}")


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------

def main() -> None:
    import uvicorn

    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")

    cfg     = ConfigLoader()
    api_cfg = cfg.load("api.yaml")
    host    = api_cfg.get("host", "0.0.0.0")
    port    = int(api_cfg.get("port", 8000))

    uvicorn.run("src.api.main:app", host=host, port=port, reload=False)


if __name__ == "__main__":
    main()
