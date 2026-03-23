# Fire Detection — YOLOv10x

Fire and smoke real-time detection using YOLOv10x, fine-tuned on the D-Fire dataset.
Full MLOps pipeline: experiment tracking (MLflow + MinIO + PostgreSQL), model registry,
live inference API (FastAPI + WebSocket), and monitoring (Prometheus + Grafana + Evidently).

---

## Table of Contents

1. [Project Structure](#project-structure)
2. [Architecture Overview](#architecture-overview)
3. [Prerequisites](#prerequisites)
4. [Quick Start](#quick-start)
5. [Full Pipeline — 12 Stages](#full-pipeline--12-stages)
6. [Dataset](#dataset)
7. [Model Weights](#model-weights)
8. [Infrastructure Services](#infrastructure-services)
9. [Inference API](#inference-api)
10. [Monitoring Stack](#monitoring-stack)
11. [Config Reference](#config-reference)
12. [Output Artifacts](#output-artifacts)

---

## Project Structure

```
Fire_Detection/
├── config/                          # YAML config files (all paths/URIs here — never hardcoded)
│   ├── app.yaml                     #   project name, environment, seed, logging level
│   ├── dataset.yaml                 #   dataset path, source, name, version
│   ├── model.yaml                   #   init weights path, model family, source repo
│   ├── training.yaml                #   epochs, batch, freeze, optimizer, lr, augmentation
│   ├── mlflow.yaml                  #   tracking URI, experiment name, MinIO endpoint
│   ├── monitoring.yaml              #   Prometheus/Grafana/Evidently URLs + thresholds
│   ├── api.yaml                     #   API host, port, WebSocket settings
│   └── environment.yaml             #   Python version, CUDA expectations
│
├── infra/
│   ├── mlflow/                      # MLflow stack (Postgres + MinIO + MLflow server)
│   │   └── docker-compose.yml
│   └── monitoring/                  # Monitoring stack (Prometheus + Grafana + Evidently)
│       ├── docker-compose.yml
│       ├── prometheus.yml           #   scrape config
│       ├── fire_alerts.yml          #   Prometheus alerting rules
│       └── grafana/
│           └── dashboards/          #   pre-provisioned Grafana dashboard JSON
│
├── src/
│   ├── core/                        # Config loader, logger, env fingerprint, git info
│   ├── dataset/                     # Metadata loader, validator, analyzer, report exporter
│   ├── model/                       # YOLO model loader
│   ├── training/                    # FireDetectionTrainer (wraps ultralytics YOLO)
│   ├── tracking/                    # MLflow client, experiment manager, artifact manager
│   ├── reporting/                   # Experiment summary generator
│   ├── monitoring/                  # Prometheus metrics (metrics.py) + Evidently HTTP client
│   ├── api/                         # FastAPI app, instrumented detector, WebSocket stream manager
│   └── pipeline/
│       ├── stages.py                # 12 concrete stage classes (BaseStage subclasses)
│       └── orchestrator.py          # PipelineOrchestrator — runs stages sequentially
│
├── data/
│   └── DFire/                       # D-Fire dataset (not committed)
│       └── data/
│           ├── train/{images,labels}/
│           ├── val/{images,labels}/
│           └── test/{images,labels}/
│
├── Model/
│   └── best.pt                      # YOLOv10x init weights from HuggingFace (not committed)
│
├── runs/train/                      # YOLO training outputs (not committed)
│   └── <run_name>/weights/
│       ├── best.pt                  # Best checkpoint (used for eval + registration)
│       └── last.pt                  # Last checkpoint (used for resume)
│
├── reports/
│   ├── evaluation/                  # Pipeline artifact outputs
│   │   ├── env_fingerprint.json
│   │   ├── init_weights_manifest.json
│   │   ├── dataset_report.md
│   │   ├── dataset_stats.json
│   │   └── evidently_reference.json
│   └── experiments/
│       └── experiment_summary.md
│
└── requirements.txt
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    PIPELINE (orchestrator.py)                   │
│                                                                 │
│  Stage 1   LoadConfig          Load all YAML configs            │
│  Stage 2   EnvFingerprint      OS/GPU/Python snapshot + git     │
│  Stage 3   InitWeightsManifest SHA256 + size of best.pt         │
│  Stage 4   DatasetValidate     YOLO layout check (D-Fire)       │
│  Stage 5   DatasetAnalyze      Class distribution + box stats   │
│  Stage 6   DatasetReportExport Markdown + JSON reports          │
│  Stage 7   MLflowSmokeRun      Framework run → MinIO artifacts  │
│  Stage 8   Train               Fine-tune YOLOv10x (ultralytics) │
│  Stage 9   ModelEvaluate       YOLO val() per-class mAP + refs  │
│  Stage 10  ExperimentSummary   Pull top runs from MLflow UI     │
│  Stage 11  ModelRegister       Register best.pt → MLflow Registry│
│  Stage 12  MonitoringCheck     Health check + Evidently upload  │
└─────────────────────────────────────────────────────────────────┘

         ┌────────────┐     ┌──────────┐     ┌──────────────┐
         │  MLflow UI │     │  MinIO   │     │  PostgreSQL  │
         │ :5000      │◄────│ :9000/01 │◄────│  :5432       │
         └────────────┘     └──────────┘     └──────────────┘

         ┌────────────┐     ┌──────────┐     ┌──────────────┐
         │ Prometheus │     │ Grafana  │     │  Evidently   │
         │ :9090      │────►│ :3000    │     │  :8001       │
         └────────────┘     └──────────┘     └──────────────┘
                  ▲
                  │ scrape /metrics
         ┌────────────────┐
         │  FastAPI / API │  :8000
         │  /metrics      │  (Prometheus exposition)
         │  /health       │
         │  /ws/{cam_id}  │  (WebSocket live stream)
         └────────────────┘
```

---

## Prerequisites

| Requirement | Version |
|-------------|---------|
| Python | 3.10+ |
| CUDA (optional) | 11.8+ |
| Docker Desktop | 4.x+ |
| GPU VRAM | 6 GB+ (RTX 3050 tested) |

```bash
pip install -r requirements.txt
```

---

## Quick Start

### Step 1 — Start MLflow infrastructure

```bash
cd infra/mlflow
docker compose up -d
```

Wait ~30 seconds, then:
- Open **MinIO console**: http://localhost:9001 (user: `minioadmin` / pass: `minioadmin`)
- Create a bucket named **`mlflow`** (required before first run)
- Open **MLflow UI**: http://localhost:5000

### Step 2 — (Optional) Start the Inference API

```bash
python -m src.api.main
```

Exposes:
- `GET  http://localhost:8000/health`     — health check
- `GET  http://localhost:8000/metrics`    — Prometheus scrape endpoint
- `WS   ws://localhost:8000/ws/{cam_id}` — live WebSocket stream

### Step 3 — (Optional) Start the Monitoring stack

```bash
cd infra/monitoring
docker compose up -d
```

- **Grafana**: http://localhost:3000 (admin / admin) — dashboard "Fire Detection — ML Monitoring"
- **Prometheus**: http://localhost:9090
- **Evidently**: http://localhost:8001 (drift detection service)

### Step 4 — Run the full pipeline

```bash
python -m src.pipeline.orchestrator
```

---

## Full Pipeline — 12 Stages

All stages share a single `ctx` dict that passes outputs between them.
Stages marked **non-fatal** log a warning on failure but do not abort the pipeline.

| # | Stage | Description | Output | Fatal? |
|---|-------|-------------|--------|--------|
| 1 | **LoadConfig** | Loads all YAML configs into `ctx` | — | Yes |
| 2 | **EnvFingerprint** | Collects OS, Python, CUDA, GPU name, git commit | `reports/evaluation/env_fingerprint.json` | Yes |
| 3 | **InitWeightsManifest** | Checks `best.pt` exists, computes SHA-256 + size | `reports/evaluation/init_weights_manifest.json` | Yes |
| 4 | **DatasetValidate** | Validates D-Fire YOLO layout (`data.yaml` + split dirs) | in-memory report | No |
| 5 | **DatasetAnalyze** | Class counts, bbox stats per split | in-memory stats | No |
| 6 | **DatasetReportExport** | Writes dataset Markdown + JSON reports | `reports/evaluation/dataset_report.md` + `dataset_stats.json` | No |
| 7 | **MLflowSmokeRun** | Logs params/metrics/artifacts to MLflow for framework verification | MLflow run (artifacts in MinIO) | No |
| 8 | **Train** | Fine-tunes YOLOv10x via `ultralytics` YOLO — all hyperparams from `training.yaml` | `runs/train/<run_name>/weights/best.pt` | Yes |
| 9 | **ModelEvaluate** | Runs `model.val()` on eval split, extracts per-class mAP50/precision/recall, samples reference frames for Evidently | `reports/evaluation/evidently_reference.json` + MLflow eval run | Yes |
| 10 | **ExperimentSummaryExport** | Pulls top-20 runs from MLflow experiment, writes summary | `reports/experiments/experiment_summary.md` | No |
| 11 | **ModelRegister** | Logs `best.pt` as MLflow artifact, registers as model version `fire-detection-yolo`, sets alias `production` | MLflow Model Registry entry | No |
| 12 | **MonitoringCheck** | Health-checks Prometheus/Grafana/Evidently; uploads reference data to Evidently if reachable | `ctx["monitoring_health"]` | No |

### Stage 8 — Train details

Controlled entirely by `config/training.yaml`:

| Key | Default | Description |
|-----|---------|-------------|
| `enabled` | `true` | Set `false` to skip training (framework-only run) |
| `epochs` | `50` | Training epochs |
| `imgsz` | `416` | Input image size |
| `batch` | `2` | Batch size (tuned for 6 GB VRAM) |
| `freeze` | `5` | Number of backbone layers to freeze |
| `optimizer` | `AdamW` | Optimizer |
| `lr0` | `0.0002` | Initial learning rate |
| `patience` | `20` | Early stopping patience |
| `resume` | `false` | Resume from last checkpoint |
| `project` | `runs/train` | Output directory (relative to project root) |

Fine-tuning strategy (progressive unfreezing):
- **Stage 1** (done): head-only, `freeze=10`, `lr0=0.0005` — mAP50 = 0.601 (init) → 0.754
- **Stage 2** (current): last 5 backbone blocks, `freeze=5`, `lr0=0.0002`
- **Stage 3** (future): full unfreeze, very low LR

### Stage 9 — ModelEvaluate details

- Runs `YOLO.val()` on the configured split (`eval_split` in `monitoring.yaml`, default `test`)
- Extracts per-class metrics: `eval.fire.mAP50`, `eval.fire.precision`, `eval.fire.recall`, same for smoke
- Logs all metrics to a dedicated MLflow `evaluate_<split>` run
- Samples up to `eval_reference_sample` (default 300) frames → produces per-frame records:
  `detections_per_frame`, `confidence_fire`, `confidence_smoke`, `fire_ratio`, `prediction`
- Saves to `reports/evaluation/evidently_reference.json`

### Stage 11 — ModelRegister details

- Finds `best.pt` from: (1) current train run → (2) most recent `runs/train/*/weights/best.pt` → (3) `model.yaml` init weights path
- Logs weights as MLflow artifact under `runs:/<run_id>/weights/best.pt`
- Registers as model name **`fire-detection-yolo`**
- Sets alias `production` (MLflow ≥ 2.x) or stage `Production` (fallback)

---

## Dataset

**D-Fire** — 21,527 annotated images (fire and smoke in outdoor environments).

| Split | Images |
|-------|--------|
| train | 14,122 |
| val   | 3,099  |
| test  | 4,306  |

**Classes** (remapped from original to match init weights):

| Class ID | Name |
|----------|------|
| 0 | fire |
| 1 | smoke |

> Original D-Fire labels had `smoke=0 / fire=1`. Labels were remapped to `fire=0 / smoke=1`
> to match the HuggingFace init weights (`TommyNgx/YOLOv10-Fire-and-Smoke-Detection`).

Expected directory layout:

```
data/DFire/data/
├── data.yaml
├── train/
│   ├── images/
│   └── labels/
├── val/
│   ├── images/
│   └── labels/
└── test/
    ├── images/
    └── labels/
```

---

## Model Weights

| Weights | Path | mAP50 | Note |
|---------|------|-------|------|
| HuggingFace init | `Model/best.pt` | 0.601 | YOLOv10x, 64 MB — classes fire=0 smoke=1 |
| Stage 1 fine-tune | `runs/train/dfire_finetune_20260304_*/weights/best.pt` | 0.754 | freeze=10, head-only |
| Stage 2 fine-tune | `runs/train/dfire_finetune_<latest>/weights/best.pt` | TBD | freeze=5, backbone partial |

Source: `TommyNgx/YOLOv10-Fire-and-Smoke-Detection` on HuggingFace.

> `Model/best.pt` and `runs/` are excluded from git (see `.gitignore`).

---

## Infrastructure Services

### MLflow Stack (`infra/mlflow/`)

| Service | URL | Credentials |
|---------|-----|-------------|
| MLflow UI | http://localhost:5000 | — |
| MinIO console | http://localhost:9001 | minioadmin / minioadmin |
| MinIO S3 API | http://localhost:9000 | minioadmin / minioadmin |
| PostgreSQL | localhost:5432 | mlflow / mlflow |

```bash
# Start
cd infra/mlflow && docker compose up -d

# Stop
cd infra/mlflow && docker compose down

# Destroy all data (volumes)
cd infra/mlflow && docker compose down -v
```

### Monitoring Stack (`infra/monitoring/`)

| Service | URL | Credentials |
|---------|-----|-------------|
| Grafana | http://localhost:3000 | admin / admin |
| Prometheus | http://localhost:9090 | — |
| Evidently | http://localhost:8001 | — |

```bash
# Start
cd infra/monitoring && docker compose up -d

# Stop
cd infra/monitoring && docker compose down
```

Prometheus scrapes:
- `host.docker.internal:8000/metrics` — FastAPI inference metrics
- `evidently:8001/metrics` — Evidently drift metrics

---

## Inference API

Start:

```bash
python -m src.api.main
```

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check — returns `{"status": "ok"}` |
| `/metrics` | GET | Prometheus scrape — exposes all `fire_*` metrics |
| `/ws/{camera_id}` | WebSocket | Live stream — send JPEG frames, receive JSON detections |

### Prometheus Metrics (`:8000/metrics`)

| Metric | Type | Description |
|--------|------|-------------|
| `fire_inference_latency_seconds` | Histogram | YOLO inference latency per frame |
| `fire_detections_total{class_name}` | Counter | Cumulative fire/smoke detections |
| `fire_detection_confidence{class_name}` | Histogram | Confidence score distribution (drift signal) |
| `fire_active_cameras` | Gauge | Active WebSocket camera connections |
| `fire_frames_processed_total{camera_id}` | Counter | Frames processed per camera |

---

## Monitoring Stack

The Grafana dashboard **"Fire Detection — ML Monitoring"** is auto-provisioned on startup and shows:

- Inference latency (p50, p95, p99)
- Detection rate by class (fire / smoke)
- Confidence distribution over time
- Active camera count
- Frame throughput

**Evidently** drift service:
- Reference data uploaded by Stage 12 (`MonitoringCheck`) after each pipeline run
- Tracks `confidence_fire`, `confidence_smoke`, `fire_ratio`, `detections_per_frame`
- Drift threshold: `0.1` (PSI/KS) — configurable in `config/monitoring.yaml`

**Alert thresholds** (`infra/monitoring/fire_alerts.yml`):

| Alert | Threshold |
|-------|-----------|
| Inference latency p95 | > 500 ms |
| API error rate | > 5% |

---

## Config Reference

| File | Key parameters |
|------|----------------|
| `config/app.yaml` | `project_name`, `environment`, `logging_level`, `seed` |
| `config/dataset.yaml` | `dataset_path`, `dataset_source`, `dataset_name`, `data_yaml_name` |
| `config/model.yaml` | `init_weights_local_path`, `init_weights_repo`, `model_family` |
| `config/training.yaml` | `enabled`, `epochs`, `batch`, `imgsz`, `freeze`, `lr0`, `optimizer`, `resume`, `project` |
| `config/mlflow.yaml` | `tracking_uri`, `experiment_name`, `minio_endpoint` |
| `config/monitoring.yaml` | `prometheus_url`, `grafana_url`, `evidently_url`, `eval_split`, `eval_conf`, `eval_iou`, `eval_reference_sample`, thresholds |
| `config/api.yaml` | `host`, `port`, WebSocket settings |
| `config/environment.yaml` | `python_version` |

---

## Output Artifacts

| Artifact | Path | Stage |
|----------|------|-------|
| Env fingerprint | `reports/evaluation/env_fingerprint.json` | 2 |
| Init weights manifest | `reports/evaluation/init_weights_manifest.json` | 3 |
| Dataset report | `reports/evaluation/dataset_report.md` | 6 |
| Dataset stats | `reports/evaluation/dataset_stats.json` | 6 |
| Best model weights | `runs/train/<run_name>/weights/best.pt` | 8 |
| Evidently reference data | `reports/evaluation/evidently_reference.json` | 9 |
| Experiment summary | `reports/experiments/experiment_summary.md` | 10 |
| MLflow Model Registry | `fire-detection-yolo` (via MLflow UI) | 11 |

---

## Recommended Startup Order

```bash
# 1. MLflow stack (always required)
cd infra/mlflow && docker compose up -d

# 2. Inference API (required for live monitoring metrics)
python -m src.api.main

# 3. Monitoring stack (optional — needed for Grafana/Evidently)
cd infra/monitoring && docker compose up -d

# 4. Run the pipeline (training + evaluation + registration + monitoring check)
python -m src.pipeline.orchestrator
```

> To run **framework-only** (skip training): set `enabled: false` in `config/training.yaml`.
