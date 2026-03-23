# Fire Detection — YOLOv10x MLOps Pipeline

Real-time fire and smoke detection using **YOLOv10x** fine-tuned on the **D-Fire** dataset (21,527 images).
Full MLOps lifecycle: experiment tracking, model registry, live inference API, and production monitoring.

| Component | Technology |
|-----------|-----------|
| Model | YOLOv10x (ultralytics) |
| Experiment Tracking | MLflow + PostgreSQL + MinIO (S3) |
| Inference API | FastAPI + WebSocket + OpenCV |
| Monitoring | Prometheus + Grafana + Evidently AI |
| CI/CD | GitHub Actions (self-hosted runner) |
| Containerization | Docker + GHCR |

---

## Table of Contents

1. [Project Structure](#1-project-structure)
2. [Architecture Overview](#2-architecture-overview)
3. [Prerequisites](#3-prerequisites)
4. [Quick Start](#4-quick-start)
5. [12-Stage Pipeline](#5-12-stage-pipeline)
6. [Dataset](#6-dataset)
7. [Model Weights & Training Results](#7-model-weights--training-results)
8. [Infrastructure Services](#8-infrastructure-services)
9. [Inference API](#9-inference-api)
10. [Monitoring Stack](#10-monitoring-stack)
11. [CI/CD Pipeline](#11-cicd-pipeline)
12. [Configuration Reference](#12-configuration-reference)
13. [Testing](#13-testing)
14. [Output Artifacts](#14-output-artifacts)

---

## 1. Project Structure

```
fire-detection/
├── .github/
│   └── workflows/
│       ├── ci.yml              # CI: lint → tests + coverage → pipeline check → summary
│       └── cd.yml              # CD: build Docker image → push to GHCR
│
├── config/                     # All runtime configuration (YAML — no hardcoded values)
│   ├── app.yaml                # Project name, environment, log level, seed
│   ├── api.yaml                # API host/port, model path, confidence thresholds
│   ├── dataset.yaml            # Dataset source (local/roboflow), path, name
│   ├── environment.yaml        # Python version requirement
│   ├── mlflow.yaml             # Tracking URI, MinIO endpoint, experiment name
│   ├── model.yaml              # Init weights source (HuggingFace), local path
│   ├── monitoring.yaml         # Prometheus/Grafana/Evidently URLs, alert thresholds
│   └── training.yaml           # Hyperparameters (epochs, batch, freeze, lr0, ...)
│
├── src/
│   ├── api/
│   │   ├── main.py             # FastAPI app — all HTTP + WebSocket endpoints
│   │   ├── detector.py         # FireDetector: YOLO inference wrapper (Prometheus instrumented)
│   │   ├── stream_manager.py   # Camera stream lifecycle + WebSocket broadcast
│   │   └── static/             # Dashboard HTML/JS (served at GET /)
│   ├── core/
│   │   ├── config_loader.py    # ConfigLoader: YAML loading with caching
│   │   ├── logger.py           # Structured logging setup
│   │   ├── env_fingerprint.py  # OS/GPU/Python environment snapshot
│   │   └── git_info.py         # Git commit hash extraction
│   ├── dataset/
│   │   ├── metadata.py         # YOLO data.yaml loader
│   │   ├── validator.py        # Dataset layout validation (images + labels)
│   │   ├── analyzer.py         # Class distribution + bounding box statistics
│   │   └── report_exporter.py  # Export Markdown + JSON reports
│   ├── model/
│   │   └── loader.py           # YOLO model factory
│   ├── training/
│   │   └── trainer.py          # FireDetectionTrainer (wraps ultralytics YOLO.train)
│   ├── tracking/
│   │   ├── mlflow_client.py    # MLflow API wrapper (runs, params, metrics, artifacts)
│   │   ├── experiment_manager.py # Experiment and run queries
│   │   └── artifact_manager.py   # Artifact upload helpers
│   ├── reporting/
│   │   └── experiment_summary.py # Top-N runs report generator
│   ├── monitoring/
│   │   ├── metrics.py          # Prometheus metric definitions
│   │   └── evidently_client.py # HTTP client for Evidently drift service
│   └── pipeline/
│       ├── stages.py           # 12 BaseStage subclasses (the full ML pipeline)
│       └── orchestrator.py     # PipelineOrchestrator (sequential runner + summary)
│
├── infra/
│   ├── mlflow/
│   │   ├── docker-compose.yml  # PostgreSQL + MinIO + MLflow server
│   │   └── Dockerfile          # MLflow server image
│   └── monitoring/
│       ├── docker-compose.yml  # Prometheus + Grafana + Evidently
│       ├── config/
│       │   ├── prometheus.yml          # Scrape config
│       │   ├── prometheus/fire_alerts.yml  # Alert rules
│       │   └── grafana/                    # Dashboard + datasource provisioning
│       └── evidently/
│           ├── Dockerfile      # Evidently drift service image
│           └── main.py         # Flask API (reference upload, drift analysis)
│
├── tests/
│   ├── test_config.py          # 9 tests — ConfigLoader (no GPU/MLflow required)
│   └── test_pipeline_stages.py # 12 tests — Stages 1–3 framework validation
│
├── scripts/
│   ├── download_dfire.py       # Download D-Fire dataset from Kaggle
│   ├── generate_doc.py         # Doc generation utility
│   └── setup_runner.ps1        # PowerShell: install GitHub Actions self-hosted runner
│
├── docs/
│   └── Fire_Detection_System_Design.docx
│
├── data/DFire/                 # D-Fire dataset (gitignored)
├── runs/train/                 # YOLO training outputs (gitignored)
├── Model/best.pt               # YOLOv10x init weights (gitignored)
├── reports/                    # Generated reports (committed)
├── Dockerfile                  # Fire API container image
├── .env.example                # Environment secrets template
├── requirements.txt
└── ruff.toml                   # Python linter configuration
```

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    ML PIPELINE (12 STAGES)                      │
│                                                                 │
│  LoadConfig → EnvFingerprint → InitWeightsManifest             │
│       → DatasetValidate → DatasetAnalyze → DatasetReportExport │
│       → MLflowSmokeRun → Train → ModelEvaluate                 │
│       → ExperimentSummaryExport → ModelRegister                │
│       → MonitoringCheck                                         │
└────────────────────────┬────────────────────────────────────────┘
                         │ registers model
                         ▼
┌──────────────────────────────────┐
│  MLflow Registry (PostgreSQL)    │
│  Artifacts → MinIO (S3)          │
│  Model: fire-detection-yolo v9   │
│  Alias: production               │
└──────────────┬───────────────────┘
               │ model weights
               ▼
┌──────────────────────────────────┐     ┌─────────────────────────┐
│  FastAPI Inference Server        │────▶│  Prometheus :9090       │
│  :8000                           │     │  Grafana    :3000       │
│  GET  /health                    │     │  Evidently  :8001       │
│  GET  /metrics (Prometheus)      │     └─────────────────────────┘
│  WS   /ws/{camera_id}            │
│  POST /api/cameras  (RTSP/webcam)│
│  GET  /            (dashboard)   │
└──────────────────────────────────┘
```

**Data flow for live inference:**
1. Camera stream (RTSP or webcam) → `StreamManager` pulls frames via OpenCV
2. Each frame → `FireDetector.detect()` → YOLO inference → bounding boxes
3. Results broadcast to connected WebSocket clients (browser dashboard)
4. Metrics (latency, confidence, detection counts) → Prometheus `/metrics`
5. Confidence samples → Evidently (drift detection)

---

## 3. Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Python | 3.10+ | 3.11 recommended |
| CUDA | 11.8+ | Optional — CPU inference supported |
| GPU VRAM | 6 GB+ | RTX 3050 minimum for training (batch=8, imgsz=320) |
| Docker Desktop | 4.x+ | For MLflow and monitoring stacks |
| Git | any | For git info collection |

Install Python dependencies:

```bash
pip install -r requirements.txt
```

For training (not required for inference):
```bash
pip install torch torchvision --index-url https://download.pytorch.org/whl/cu118
pip install ultralytics>=8.2
```

---

## 4. Quick Start

### Step 1 — Start MLflow stack

```bash
cd infra/mlflow
docker compose up -d
```

Waits for PostgreSQL + MinIO to be healthy before starting MLflow server.

| Service | URL | Credentials |
|---------|-----|-------------|
| MLflow UI | http://localhost:5000 | — |
| MinIO Console | http://localhost:9001 | minioadmin / minioadmin |
| PostgreSQL | localhost:5432 | mlflow / mlflow |

### Step 2 — Start monitoring stack

```bash
cd infra/monitoring
docker compose up -d
```

| Service | URL | Credentials |
|---------|-----|-------------|
| Grafana | http://localhost:3000 | admin / admin |
| Prometheus | http://localhost:9090 | — |
| Evidently | http://localhost:8001 | — |

### Step 3 — Run the 12-stage pipeline

```bash
python -m src.pipeline.orchestrator
```

> **Important:** Do NOT run the API and pipeline simultaneously — causes OOM on 6 GB VRAM GPUs.
> Training is disabled by default (`config/training.yaml: enabled: false`).

### Step 4 — Start the inference API

```bash
python -m src.api.main
# or
uvicorn src.api.main:app --host 0.0.0.0 --port 8000
```

Open the live dashboard: **http://localhost:8000**

### Step 5 — (Optional) Download the D-Fire dataset

Requires Kaggle credentials at `~/.kaggle/kaggle.json`:

```bash
python scripts/download_dfire.py
```

---

## 5. 12-Stage Pipeline

Run with:
```bash
python -m src.pipeline.orchestrator
```

| # | Stage | Output | Fatal |
|---|-------|--------|-------|
| 1 | **LoadConfig** | All YAML configs loaded into context | Yes |
| 2 | **EnvFingerprint** | `reports/evaluation/env_fingerprint.json` | Yes |
| 3 | **InitWeightsManifest** | `reports/evaluation/init_weights_manifest.json` | Yes |
| 4 | **DatasetValidate** | Validates D-Fire layout (images/labels per split) | No |
| 5 | **DatasetAnalyze** | Class distribution + bbox statistics | No |
| 6 | **DatasetReportExport** | `reports/evaluation/dataset_report.md` + `dataset_stats.json` | No |
| 7 | **MLflowSmokeRun** | Logs framework smoke run to MLflow with all params + artifacts | No |
| 8 | **Train** | `runs/train/<run>/weights/best.pt` — skipped if `enabled: false` | Yes |
| 9 | **ModelEvaluate** | Per-class metrics (mAP50, precision, recall) + `evidently_reference.json` | Yes |
| 10 | **ExperimentSummaryExport** | `reports/experiments/experiment_summary.md` (top-20 runs) | No |
| 11 | **ModelRegister** | Registers `best.pt` to MLflow as `fire-detection-yolo`, alias `production` | No |
| 12 | **MonitoringCheck** | Health-checks Prometheus/Grafana/Evidently; uploads reference data | No |

**Fatal** means the pipeline aborts if the stage fails. Non-fatal stages log warnings and continue.

### Stage 8 — Training Details

Controlled entirely by `config/training.yaml`. Set `enabled: true` to run training.

**Progressive unfreezing strategy:**

| Stage | Freeze Layers | lr0 | Epochs | mAP50 (val) |
|-------|--------------|-----|--------|-------------|
| Init weights (HuggingFace) | — | — | — | 0.601 |
| Stage 1 fine-tune | 10 (head-only) | 0.0005 | 20 | **0.754** |
| Stage 2 fine-tune | 5 (partial backbone) | 0.0002 | 20 | **0.722** |

Stage 2 uses Stage 1 `best.pt` as init weights (`config/model.yaml: init_weights_local_path`).

### Stage 9 — Evaluation Details

- Runs `YOLO.val()` on the configured split (default: `test`)
- Logs per-class metrics to MLflow: `eval.fire.mAP50`, `eval.smoke.mAP50`, precision, recall
- Samples up to 300 test frames → generates Evidently reference data:
  - `confidence_fire`, `confidence_smoke`, `detections_per_frame`, `fire_ratio`, `prediction`
- Output: `reports/evaluation/evidently_reference.json`

### Stage 11 — Model Registration

Best weights discovery order:
1. Current training run output (`runs/train/<current>/weights/best.pt`)
2. Most recent `runs/train/*/weights/best.pt` by modification time
3. Fallback to init weights

Registers to MLflow Model Registry as **`fire-detection-yolo`**, sets alias `production`.

---

## 6. Dataset

**D-Fire** — open-source fire and smoke detection dataset.

| Split | Images |
|-------|--------|
| Train | 14,122 |
| Val | 3,099 |
| Test | 4,306 |
| **Total** | **21,527** |

**Classes:** `fire` (0), `smoke` (1)

> Labels were remapped from the original D-Fire convention (smoke=0, fire=1) to match the
> HuggingFace YOLOv10x init weights (fire=0, smoke=1).

**Dataset structure:**
```
data/DFire/
├── data.yaml            # YOLO config (path, train/val/test, nc=2, names)
└── data/
    ├── train/
    │   ├── images/      # 14,122 .jpg files
    │   └── labels/      # .txt YOLO format: <class_id> <cx> <cy> <w> <h>
    ├── val/
    │   ├── images/      # 3,099 files
    │   └── labels/
    └── test/
        ├── images/      # 4,306 files
        └── labels/
```

---

## 7. Model Weights & Training Results

| Weights | Path | mAP50 (val) | mAP50 (test) |
|---------|------|-------------|--------------|
| Init (HuggingFace YOLOv10x) | `Model/best.pt` | 0.601 | — |
| Stage 1 fine-tune | `runs/train/dfire_finetune_20260304_.../weights/best.pt` | **0.754** | — |
| Stage 2 fine-tune (**production**) | `runs/train/dfire_finetune_20260323_.../weights/best.pt` | 0.722 | **0.731** |

**Current production model**: Stage 2 — `mAP50-95=0.458`, Fire recall=0.602, Smoke recall=0.730

MLflow registry: model name `fire-detection-yolo`, version 9, alias `production`

The production model path is configured in `config/api.yaml: model_path`.

---

## 8. Infrastructure Services

### MLflow Stack (`infra/mlflow/`)

```bash
cd infra/mlflow && docker compose up -d    # start
cd infra/mlflow && docker compose down     # stop
```

Services:
- **PostgreSQL 15** — MLflow backend store (experiment metadata, run params/metrics)
- **MinIO** — S3-compatible artifact store (model weights, reports, plots)
- **MLflow Server** — UI + REST API at `:5000`

### Monitoring Stack (`infra/monitoring/`)

```bash
cd infra/monitoring && docker compose up -d    # start
cd infra/monitoring && docker compose down     # stop
```

Services:
- **Prometheus** — scrapes `:8000/metrics` (Fire API) + `:8001/metrics` (Evidently)
- **Grafana** — pre-provisioned dashboard "Fire Detection — ML Monitoring"
- **Evidently** — drift detection service (PSI/KS on confidence distributions)

### Environment Variables

Copy `.env.example` to `.env` and configure:

```bash
# MinIO / S3 credentials for MLflow artifact store
MINIO_ACCESS_KEY=minioadmin
MINIO_SECRET_KEY=minioadmin

# API bearer token — protects POST/PUT/DELETE endpoints
API_KEY=your_secret_key_here
```

---

## 9. Inference API

Base URL: `http://localhost:8000`

### Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/` | — | Live dashboard (HTML) |
| `GET` | `/health` | — | Health check — model loaded, active cameras |
| `GET` | `/metrics` | — | Prometheus scrape endpoint |
| `GET` | `/api/cameras` | — | List all active camera streams |
| `POST` | `/api/cameras` | Bearer | Add camera (RTSP URL or webcam index) |
| `DELETE` | `/api/cameras/{id}` | Bearer | Remove camera stream |
| `GET` | `/api/webcams` | — | Scan and list available webcam devices (0–9) |
| `GET` | `/api/config` | — | Current confidence/IOU thresholds |
| `PUT` | `/api/config` | Bearer | Update confidence threshold |
| `WS` | `/ws/{camera_id}` | — | Live stream — frames + detection events |
| `POST` | `/predict` | — | Feature-vector predict (monitoring simulator) |

### Add a Camera (example)

```bash
# RTSP camera
curl -X POST http://localhost:8000/api/cameras \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"type": "rtsp", "url": "rtsp://192.168.1.100:554/stream", "name": "Cam 1"}'

# Webcam
curl -X POST http://localhost:8000/api/cameras \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"type": "webcam", "device_id": 0, "name": "Local Webcam"}'
```

### WebSocket Stream

Connect to `ws://localhost:8000/ws/{camera_id}` to receive JSON frames:

```json
{
  "type": "frame",
  "camera_id": "abc12345",
  "timestamp": 1711234567.89,
  "jpeg_b64": "<base64-encoded JPEG>",
  "detections": [
    {"class": "fire", "confidence": 0.87, "bbox": [x1, y1, x2, y2]},
    {"class": "smoke", "confidence": 0.72, "bbox": [x1, y1, x2, y2]}
  ]
}
```

### Authentication

Set `API_KEY` in `.env` to enable Bearer token authentication on write endpoints (`POST`, `PUT`, `DELETE`).
Leave `API_KEY` empty for open access (development mode).

---

## 10. Monitoring Stack

### Prometheus Metrics (`GET /metrics`)

All metrics use the `fire_` prefix:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `fire_inference_latency_seconds` | Histogram | — | YOLO inference time per frame |
| `fire_detections_total` | Counter | `class_name` | Total fire/smoke detections |
| `fire_detection_confidence` | Histogram | `class_name` | Confidence score distribution (drift signal) |
| `fire_detections_per_frame` | Histogram | — | Detections per processed frame |
| `fire_frames_processed_total` | Counter | `camera_id` | Frames processed per camera |
| `fire_inference_errors_total` | Counter | `error_type` | Inference errors |
| `fire_alerts_total` | Counter | `class_name`, `camera_id` | Alert events |
| `fire_active_cameras` | Gauge | — | Currently active camera streams |
| `fire_model` | Info | `weights_path`, `conf_threshold`, `iou_threshold` | Model metadata |
| `fire_api_requests_total` | Counter | `method`, `endpoint`, `status` | HTTP request counts |
| `fire_api_request_latency_seconds` | Histogram | `method`, `endpoint` | HTTP latency |

### Grafana Dashboard

Pre-provisioned dashboard **"Fire Detection — ML Monitoring"** at http://localhost:3000.

Panels:
- Inference latency (p50/p95/p99)
- Fire / smoke detection rate
- Active cameras
- API request rate and error rate
- Confidence distribution (fire + smoke)

### Evidently Drift Detection

The Evidently service monitors distribution drift on:
- `confidence_fire` — fire detection confidence
- `confidence_smoke` — smoke detection confidence
- `detections_per_frame` — frame-level detection density
- `fire_ratio` — proportion of fire frames

Reference data is uploaded during Stage 12 of the pipeline (sampled from test set).
The API sends production samples to Evidently at a configurable rate (`evidently_capture_sample_rate: 0.05`).

### Alert Rules (Prometheus)

Configured in `infra/monitoring/config/prometheus/fire_alerts.yml`:
- Inference p95 latency > 500 ms
- API error rate > 5%

---

## 11. CI/CD Pipeline

### CI (`ci.yml`) — runs on every push

```
Lint (ruff)
    │
    ├──> Tests + Coverage (pytest --cov=src)
    │       └── Upload: coverage.xml
    │
    └──> Pipeline check (python -m src.pipeline.orchestrator)
            └── Upload: reports/
                    │
                    └──> Build Summary (pass/fail)
```

All jobs run on a **self-hosted Windows runner** (`self-hosted,Windows,x64,gpu`) with Git Bash.

**Setup self-hosted runner:**
```powershell
# Run as Administrator, with a registration token from:
# https://github.com/MrNMD2002/detectfire/settings/actions/runners
.\scripts\setup_runner.ps1 -Token <REGISTRATION_TOKEN>
```

### CD (`cd.yml`) — runs on push to `main`

Builds the Docker image and pushes to GitHub Container Registry (GHCR):

```
actions/checkout
    → Set lowercase image name (GHCR requirement)
    → docker/setup-buildx-action
    → docker/login-action → ghcr.io
    → docker/metadata-action (tags: sha-*, latest)
    → docker/build-push-action
```

**Pull the published image:**
```bash
docker pull ghcr.io/mrnmd2002/detectfire/fire-detection-api:latest
```

**Run the published image** (mount model weights at runtime):
```bash
docker run -d \
  -p 8000:8000 \
  -v /path/to/best.pt:/app/checkpoints/best.pt \
  -e API_KEY=your_key \
  ghcr.io/mrnmd2002/detectfire/fire-detection-api:latest
```

---

## 12. Configuration Reference

### `config/training.yaml`

| Key | Default | Description |
|-----|---------|-------------|
| `enabled` | `false` | Set `true` to run training in Stage 8 |
| `epochs` | `20` | Training epochs |
| `batch` | `8` | Batch size (adjust for VRAM) |
| `imgsz` | `320` | Input image size |
| `freeze` | `5` | Number of layers to freeze (0 = full fine-tune) |
| `lr0` | `0.0002` | Initial learning rate |
| `device` | `"0"` | GPU device index (`"cpu"` for CPU) |
| `resume` | `false` | Resume from checkpoint |
| `resume_from` | `null` | Path to checkpoint `.pt` |

### `config/api.yaml`

| Key | Default | Description |
|-----|---------|-------------|
| `model_path` | Stage 2 `best.pt` | Path to YOLO weights file |
| `confidence_threshold` | `0.40` | Detection confidence cutoff |
| `iou_threshold` | `0.50` | NMS IOU threshold |
| `target_fps` | `15` | Target stream FPS |
| `host` | `0.0.0.0` | API bind address |
| `port` | `8000` | API port |

### `config/monitoring.yaml`

| Key | Default | Description |
|-----|---------|-------------|
| `evidently_capture_sample_rate` | `0.05` | % of frames sent to Evidently |
| `evidently_drift_threshold` | `0.1` | PSI/KS drift threshold |
| `inference_latency_p95_ms` | `500` | Alert threshold (ms) |
| `eval_workers` | `0` | DataLoader workers for evaluation (0 = main process) |

---

## 13. Testing

```bash
# Run all tests
pytest tests/ -v

# Run with coverage report
pytest tests/ -v --cov=src --cov-report=term-missing

# Run only config tests
pytest tests/test_config.py -v

# Run only pipeline stage tests
pytest tests/test_pipeline_stages.py -v
```

**Test suite (24 tests total):**

| File | Tests | Covers |
|------|-------|--------|
| `test_config.py` | 9 | ConfigLoader: load, validate keys, path resolution, caching, error handling |
| `test_pipeline_stages.py` | 12 | Stages 1–3: LoadConfig, EnvFingerprint, InitWeightsManifest |

All tests are **framework-only** — no GPU, no running MLflow server, no dataset required.
Tests can run in CI on any machine.

---

## 14. Output Artifacts

| Artifact | Path | Generated by |
|----------|------|--------------|
| Environment fingerprint | `reports/evaluation/env_fingerprint.json` | Stage 2 |
| Init weights manifest | `reports/evaluation/init_weights_manifest.json` | Stage 3 |
| Dataset report (Markdown) | `reports/evaluation/dataset_report.md` | Stage 6 |
| Dataset statistics (JSON) | `reports/evaluation/dataset_stats.json` | Stage 6 |
| Evidently reference data | `reports/evaluation/evidently_reference.json` | Stage 9 |
| Experiment summary | `reports/experiments/experiment_summary.md` | Stage 10 |
| YOLO training outputs | `runs/train/<run_name>/` | Stage 8 |
| MLflow artifacts | MinIO bucket `mlflow/` | Stages 7, 9, 11 |
| Coverage report | `coverage.xml` | CI |

---

## Startup Order

```bash
# 1. Start MLflow stack (PostgreSQL + MinIO + MLflow)
cd infra/mlflow && docker compose up -d

# 2. Start monitoring stack (Prometheus + Grafana + Evidently)
cd infra/monitoring && docker compose up -d

# 3. Run the 12-stage ML pipeline (without API running — OOM risk on 6 GB VRAM)
cd /path/to/fire-detection
python -m src.pipeline.orchestrator

# 4. Start the inference API (after pipeline completes)
python -m src.api.main
```

Open http://localhost:8000 to access the live dashboard.

---

## Repository

- **GitHub**: https://github.com/MrNMD2002/detectfire
- **Container Registry**: `ghcr.io/mrnmd2002/detectfire/fire-detection-api`
- **MLflow Experiment**: `fire-detection`
- **Model Registry**: `fire-detection-yolo` (alias: `production`)
