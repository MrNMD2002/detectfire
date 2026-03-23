"""
Prometheus metrics for the Fire Detection system.

All metrics are module-level singletons — import and use wherever needed.
Thread-safe: prometheus_client counters/histograms are goroutine-safe by design.
"""
from prometheus_client import Counter, Gauge, Histogram, Info

# ── Inference ──────────────────────────────────────────────────────────────────

INFERENCE_LATENCY = Histogram(
    "fire_inference_latency_seconds",
    "YOLO inference latency per frame (seconds)",
    buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.5, 1.0],
)

FRAMES_PROCESSED = Counter(
    "fire_frames_processed_total",
    "Total frames submitted to YOLO inference",
    ["camera_id"],
)

INFERENCE_ERRORS = Counter(
    "fire_inference_errors_total",
    "Total YOLO inference errors",
    ["error_type"],
)

# ── Detections ─────────────────────────────────────────────────────────────────

DETECTIONS_TOTAL = Counter(
    "fire_detections_total",
    "Total object detections by class",
    ["class_name"],   # fire | smoke
)

DETECTION_CONFIDENCE = Histogram(
    "fire_detection_confidence",
    "Confidence score distribution by class (drift-critical signal)",
    ["class_name"],
    buckets=[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
)

DETECTIONS_PER_FRAME = Histogram(
    "fire_detections_per_frame",
    "Number of objects detected per frame",
    buckets=[0, 1, 2, 3, 4, 5, 10],
)

# ── Alerts ─────────────────────────────────────────────────────────────────────

ALERTS_TOTAL = Counter(
    "fire_alerts_total",
    "Total alert events triggered",
    ["class_name", "camera_id"],
)

# ── Cameras / System ───────────────────────────────────────────────────────────

ACTIVE_CAMERAS = Gauge(
    "fire_active_cameras",
    "Number of active camera streams",
)

MODEL_INFO = Info(
    "fire_model",
    "Current model information (weights path, version, mAP50)",
)

# ── API requests ───────────────────────────────────────────────────────────────

API_REQUESTS_TOTAL = Counter(
    "fire_api_requests_total",
    "Total HTTP requests handled by the API",
    ["method", "endpoint", "status"],
)

API_REQUEST_LATENCY = Histogram(
    "fire_api_request_latency_seconds",
    "HTTP request latency in seconds",
    ["method", "endpoint"],
    buckets=[0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0],
)
