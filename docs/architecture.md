# Fire & Smoke Detection System - Architecture

## Tổng Quan

Hệ thống phát hiện cháy/khói real-time từ **10-18 camera RTSP**, sử dụng **GPU RTX3060**, quản lý qua **Web UI**, gom log **Loki/Grafana**, và gửi cảnh báo **Telegram**.

## Kiến Trúc Chi Tiết

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           FIRE & SMOKE DETECTION SYSTEM                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                         │
│  │  Camera 1   │  │  Camera 2   │  │  Camera N   │   RTSP Cameras          │
│  │   (RTSP)    │  │   (RTSP)    │  │   (RTSP)    │   (10-18 streams)       │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘                         │
│         │                │                │                                  │
│         └────────────────┼────────────────┘                                  │
│                          ▼                                                   │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                    DETECTOR SERVICE (Rust)                             │  │
│  │                                                                        │  │
│  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐                  │  │
│  │  │  GStreamer  │──▶│   Queue(1)  │──▶│   Sampler   │                  │  │
│  │  │   Pipeline  │   │  Drop Old   │   │  (2-5 FPS)  │                  │  │
│  │  └─────────────┘   └─────────────┘   └──────┬──────┘                  │  │
│  │                                             │                          │  │
│  │                                             ▼                          │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │                    INFERENCE ENGINE                              │  │  │
│  │  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐            │  │  │
│  │  │  │  Preprocess │──▶│   YOLOv10   │──▶│ Postprocess │            │  │  │
│  │  │  │  Letterbox  │   │   (ONNX)    │   │    NMS      │            │  │  │
│  │  │  │  Normalize  │   │  GPU/CUDA   │   │   Decode    │            │  │  │
│  │  │  └─────────────┘   └─────────────┘   └──────┬──────┘            │  │  │
│  │  └────────────────────────────────────────────┬────────────────────┘  │  │
│  │                                               │                        │  │
│  │                                               ▼                        │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │                    DECISION ENGINE                               │  │  │
│  │  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐            │  │  │
│  │  │  │  Sliding    │──▶│  Threshold  │──▶│  Cooldown   │            │  │  │
│  │  │  │  Window     │   │   Check     │   │  Manager    │            │  │  │
│  │  │  └─────────────┘   └─────────────┘   └──────┬──────┘            │  │  │
│  │  └────────────────────────────────────────────┬────────────────────┘  │  │
│  │                                               │                        │  │
│  │                                               ▼                        │  │
│  │  ┌─────────────────────────────────────────────────────────────────┐  │  │
│  │  │  EVENT PUBLISHER  │────────────────▶│ gRPC Stream to API        │  │  │
│  │  └─────────────────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                          │                                                   │
│                          │ gRPC (port 50051)                                │
│                          ▼                                                   │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                     API SERVICE (Rust + Axum)                          │  │
│  │                                                                        │  │
│  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐                  │  │
│  │  │  REST API   │   │  WebSocket  │   │  Telegram   │                  │  │
│  │  │  (CRUD)     │   │  Broadcast  │   │    Bot      │                  │  │
│  │  └──────┬──────┘   └──────┬──────┘   └──────┬──────┘                  │  │
│  │         │                 │                 │                          │  │
│  │         ▼                 ▼                 ▼                          │  │
│  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐                  │  │
│  │  │ PostgreSQL  │   │  Web UI     │   │  Telegram   │                  │  │
│  │  │  Database   │   │  Clients    │   │   Users     │                  │  │
│  │  └─────────────┘   └─────────────┘   └─────────────┘                  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                          │                                                   │
│                          │ HTTP (port 8080)                                 │
│                          ▼                                                   │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                      WEB UI (React + Vite)                             │  │
│  │                                                                        │  │
│  │  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐   ┌───────────┐  │  │
│  │  │  Dashboard  │   │  Cameras    │   │   Events    │   │ Settings  │  │  │
│  │  │   (Stats)   │   │   (Grid)    │   │  (Timeline) │   │  (Config) │  │  │
│  │  └─────────────┘   └─────────────┘   └─────────────┘   └───────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Các Components

### 1. Detector Service (Rust)

**Chức năng:**

- Ingest RTSP streams từ nhiều cameras
- Sample frames theo cấu hình FPS
- Chạy inference YOLOv10 trên GPU
- Decision logic với sliding window + cooldown
- Publish events qua gRPC

**Tech Stack:**

- Rust (async với Tokio)
- GStreamer (RTSP decoding)
- ONNX Runtime (GPU inference)
- Tonic (gRPC)

### 2. API Service (Rust + Axum)

**Chức năng:**

- REST API cho CRUD cameras, events
- WebSocket broadcast real-time events
- JWT authentication
- Telegram notifications
- AES-256 encryption cho RTSP URLs

**Endpoints:**

- `POST /api/auth/login`
- `GET/POST /api/cameras`
- `GET /api/events`
- `WS /ws/events`

### 3. Web UI (React + Vite)

**Chức năng:**

- Dashboard với real-time stats
- Camera grid management
- Event timeline với filtering
- Settings configuration

**Tech Stack:**

- React 18 + TypeScript
- Vite (build tool)
- Zustand (state management)
- TanStack Query (data fetching)

### 4. Monitoring Stack

**Components:**

- **Promtail**: Thu thập logs từ Docker containers
- **Loki**: Log aggregation
- **Grafana**: Visualization và alerting

## Data Flow

```
Camera → GStreamer → Queue(1) → Sampler → Preprocess → ONNX → Postprocess
                                                                    ↓
                                                            Decision Engine
                                                                    ↓
                                                            Event Publisher
                                                                    ↓
                                                          gRPC → API Service
                                                                    ↓
                                              ┌─────────────────────┼─────────────────────┐
                                              ↓                     ↓                     ↓
                                         PostgreSQL            WebSocket             Telegram
                                              ↓                     ↓                     ↓
                                         Persistence          Real-time UI            Alerts
```

## Scaling

| Cameras | GPU Memory | FPS Sample | Notes              |
| ------- | ---------- | ---------- | ------------------ |
| 10      | ~4GB       | 3          | Comfortable        |
| 15      | ~6GB       | 2-3        | Optimal            |
| 18      | ~8GB       | 2          | Near RTX3060 limit |

## Security

1. **RTSP URLs**: AES-256 encrypted in database
2. **API**: JWT authentication với 24h expiry
3. **Telegram**: Rate limited (1 msg/camera/cooldown)
4. **Network**: Internal only, expose via reverse proxy
