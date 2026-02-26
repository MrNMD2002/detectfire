# Fire & Smoke Detection System

Hệ thống phát hiện cháy và khói real-time sử dụng AI (YOLOv8/ONNX), GStreamer RTSP, và MJPEG streaming.
Được xây dựng bằng **Rust** (backend + detector), **React/TypeScript** (frontend), **PostgreSQL**, và **Docker**.

---

## Mục lục

- [Kiến trúc hệ thống](#kiến-trúc-hệ-thống)
- [Tech Stack](#tech-stack)
- [Yêu cầu hệ thống](#yêu-cầu-hệ-thống)
- [Cấu trúc thư mục](#cấu-trúc-thư-mục)
- [Cài đặt & Chạy (Docker)](#cài-đặt--chạy-docker)
  - [GPU (CUDA)](#1-chế-độ-gpu-cuda---khuyến-nghị)
  - [CPU](#2-chế-độ-cpu-không-cần-gpu)
  - [Dev infra only](#3-dev---chỉ-chạy-infra-postgres)
- [Build từng service](#build-từng-service)
- [Cấu hình](#cấu-hình)
  - [.env](#deploymentenv)
  - [cameras.yaml](#configscamerasyaml)
  - [settings.yaml](#configssettingsyaml)
- [Database Migrations](#database-migrations)
- [Development (local)](#development-local)
- [API Endpoints](#api-endpoints)
- [Luồng dữ liệu & Stream](#luồng-dữ-liệu--stream)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)
- [Tài khoản mặc định](#tài-khoản-mặc-định)

---

## Kiến trúc hệ thống

```
┌─────────────────┐     RTSP      ┌──────────────────────────────────────┐
│  RTSP Cameras   │──────────────▶│            Detector Service          │
│  (H.264/H.265)  │               │  GStreamer → ONNX (YOLO) → Decision  │
└─────────────────┘               │                                      │
                                  │  broadcast::Sender<Arc<Frame>>       │
                                  │         │            │               │
                                  │    MJPEG HTTP    gRPC :50051         │
                                  │    :51051        Events              │
                                  └──────────────────────────────────────┘
                                            │              │
                                    MJPEG proxy       gRPC stream
                                            │              │
                              ┌─────────────▼──────────────▼────────────┐
                              │              API Service                 │
                              │        Axum REST :8080                   │
                              │   JWT Auth / Camera CRUD / Events        │
                              │   SQLx + PostgreSQL                      │
                              └──────────┬─────────────────┬────────────┘
                                         │                 │
                                    REST/WS           Telegram Bot
                                         │
                              ┌──────────▼──────────┐
                              │     Web (nginx)      │
                              │   React SPA :80/8081 │
                              └─────────────────────┘
```

**Luồng stream MJPEG (push model, ~200ms latency):**
```
Camera RTSP → GStreamer (decode H.264/H.265) → appsink callback
  → broadcast::Sender<Arc<Frame>>  (zero-copy, shared giữa inference + stream)
  → MJPEG HTTP server (port 51051)
  → API proxy /api/cameras/{id}/stream/mjpeg  (auth JWT)
  → nginx (proxy_buffering off)
  → Browser <img src="...">
```

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Detector | Rust, GStreamer 1.24, ONNX Runtime 1.23.2 (CUDA/CPU), Tokio |
| API | Rust, Axum 0.7, SQLx 0.7, tonic (gRPC), Axum WebSocket |
| Frontend | React 18, TypeScript, Vite 5, TanStack Query, Zustand, Recharts |
| Database | PostgreSQL 16, AES-256-GCM (mã hóa RTSP URL) |
| Inference | YOLOv8 ONNX (fire / smoke / other), GPU CUDA 12 |
| Deploy | Docker Compose, nginx (reverse proxy) |
| Monitoring | Prometheus, Grafana, Loki, Promtail |

---

## Yêu cầu hệ thống

### Chạy bằng Docker (khuyến nghị)

| Thành phần | Yêu cầu |
|-----------|---------|
| OS | Ubuntu 22.04+ hoặc Windows 11 + WSL2 |
| Docker | 24.0+ |
| Docker Compose | 2.20+ |
| RAM | Tối thiểu 8 GB (16 GB khuyến nghị) |
| Disk | 10 GB trống (images + models + snapshots) |

**Chế độ GPU (thêm):**

| Thành phần | Yêu cầu |
|-----------|---------|
| GPU | NVIDIA RTX 3060 hoặc tốt hơn (6 GB VRAM+) |
| NVIDIA Driver | 525+ |
| NVIDIA Container Toolkit | [Cài đặt tại đây](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) |

### Build local (development)

- Rust 1.88+
- Node.js 20+
- GStreamer 1.24 dev libraries
- protobuf-compiler
- PostgreSQL 16

---

## Cấu trúc thư mục

```
fire-detect/
├── apps/
│   ├── api/                    # REST API (Rust/Axum)
│   │   ├── src/
│   │   │   ├── main.rs         # Entry point, startup sequence
│   │   │   ├── routes/         # auth, cameras, events, stream, snapshots, settings
│   │   │   ├── db/             # SQLx queries
│   │   │   ├── camera_sync.rs  # Ghi cameras.yaml từ DB
│   │   │   ├── detector_client.rs  # gRPC client tới detector
│   │   │   └── telegram.rs     # Telegram bot integration
│   │   ├── migrations/         # SQL migrations (5 files)
│   │   └── .sqlx/              # SQLx offline query cache
│   │
│   ├── detector/               # Detection service (Rust)
│   │   └── src/
│   │       ├── camera/         # GStreamer RTSP pipeline, worker, manager
│   │       ├── inference/      # ONNX Runtime YOLO inference
│   │       ├── decision/       # Sliding window + cooldown engine
│   │       ├── event/          # Event publishing + gRPC server
│   │       └── stream/         # MJPEG push server
│   │
│   └── web/                    # React frontend
│       └── src/
│           ├── pages/          # Dashboard, Cameras, Events, Settings, Login
│           ├── components/     # CameraStreamModal (MJPEG viewer)
│           ├── hooks/          # useWebSocket
│           └── lib/api.ts      # Axios API client
│
├── configs/
│   ├── cameras.yaml            # Cấu hình camera (camera_id, RTSP URL, codec, ...)
│   └── settings.yaml           # Cấu hình hệ thống (inference, DB, Telegram, ...)
│
├── models/
│   ├── best.onnx               # Model ONNX đã export (38 MB)
│   └── best.pt                 # YOLO weights gốc (20 MB)
│
└── deploy/
    └── docker/
        ├── .env                # Biến môi trường (tạo từ .env.example)
        ├── docker-compose.yml              # Stack mặc định (GPU)
        ├── docker-compose.prod-gpu.yml     # Production GPU
        ├── docker-compose.prod-cpu.yml     # Production CPU
        ├── docker-compose.dev-infra.yml    # Chỉ postgres (dev)
        ├── Dockerfile.detector             # GPU (CUDA 12)
        ├── Dockerfile.detector.cpu         # CPU only
        ├── Dockerfile.api                  # API service
        ├── Dockerfile.web                  # React + nginx
        └── nginx.conf                      # Reverse proxy config
```

---

## Cài đặt & Chạy (Docker)

### Bước 1: Clone và chuẩn bị

```bash
git clone <repo-url> fire-detect
cd fire-detect
```

### Bước 2: Tạo file .env

```bash
cp deploy/docker/.env.example deploy/docker/.env
# Chỉnh sửa .env theo hướng dẫn bên dưới
```

> Xem chi tiết cấu hình tại mục [.env](#deploymentenv)

### Bước 3: Chuẩn bị model ONNX

Đảm bảo file model tồn tại:

```bash
ls models/best.onnx    # ~38 MB
```

Nếu chưa có, export từ PyTorch:

```bash
pip install -r models/requirements.txt
python models/export_onnx.py
```

---

### 1. Chế độ GPU (CUDA) — Khuyến nghị

**Yêu cầu:** NVIDIA GPU + NVIDIA Container Toolkit đã cài.

```bash
cd deploy/docker

# Kiểm tra GPU
docker run --rm --gpus all nvidia/cuda:12.6.3-base-ubuntu24.04 nvidia-smi

# Build và start toàn bộ stack
docker compose -f docker-compose.prod-gpu.yml up -d --build

# Hoặc dùng docker-compose.yml mặc định (cũng GPU)
docker compose up -d --build
```

---

### 2. Chế độ CPU (không cần GPU)

```bash
cd deploy/docker

docker compose -f docker-compose.prod-cpu.yml up -d --build
```

> Inference sẽ chậm hơn (~10-15 FPS tùy CPU). Phù hợp cho test/dev.

---

### 3. Dev — Chỉ chạy infra (postgres)

Khi muốn chạy API/detector local, chỉ cần postgres trong Docker:

```bash
cd deploy/docker

docker compose -f docker-compose.dev-infra.yml up -d
# postgres chạy tại localhost:5432
```

---

### Kiểm tra trạng thái

```bash
cd deploy/docker

# Xem tất cả container
docker compose ps

# Logs realtime
docker compose logs -f

# Logs từng service
docker compose logs detector --tail=50
docker compose logs api --tail=50
docker compose logs web --tail=20
```

**Trạng thái bình thường:**

```
NAME                   STATUS              PORTS
fire-detect-db         Up (healthy)        0.0.0.0:5432->5432/tcp
fire-detect-detector   Up (healthy)
fire-detect-api        Up (healthy)        0.0.0.0:8080->8080/tcp
fire-detect-web        Up (healthy)        0.0.0.0:8081->80/tcp
```

### Dừng / Khởi động lại

```bash
cd deploy/docker

# Dừng tất cả (giữ data)
docker compose down

# Dừng và xóa volumes (reset DB)
docker compose down -v

# Restart một service
docker compose restart api
docker compose restart detector

# Rebuild và restart một service
docker compose up -d --build api
docker compose up -d --build detector
```

---

## Build từng service

### Build API

```bash
cd deploy/docker

# Build image API (SQLX_OFFLINE=true, không cần DB chạy)
docker build -f Dockerfile.api -t fire-detect-api:latest ../../

# Rebuild từ đầu (bỏ cache)
docker build --no-cache -f Dockerfile.api -t fire-detect-api:latest ../../
```

### Build Detector (GPU)

```bash
cd deploy/docker

docker build -f Dockerfile.detector -t fire-detect-detector:latest ../../
```

### Build Detector (CPU)

```bash
cd deploy/docker

docker build -f Dockerfile.detector.cpu -t fire-detect-detector:latest ../../
```

### Build Web

```bash
cd deploy/docker

docker build -f Dockerfile.web -t fire-detect-web:latest ../../
```

---

## Cấu hình

### `deploy/docker/.env`

Tạo file `.env` với nội dung sau (thay đổi các giá trị bắt buộc):

```env
# ── Camera RTSP URLs ─────────────────────────────────────────────────────────
# Thêm biến môi trường theo format CAM_XX_RTSP_URL cho từng camera trong cameras.yaml
CAM_01_RTSP_URL=rtsp://user:password@192.168.1.100:554/stream1

# ── Database ─────────────────────────────────────────────────────────────────
DB_HOST=postgres
DB_PORT=5432
DB_NAME=fire_detect
DB_USER=fire_detect
DB_PASSWORD=change-me-strong-password

# QUAN TRỌNG: Key mã hóa RTSP URLs trong DB (AES-256-GCM)
# Phải đúng 32 ký tự. Không được thay đổi sau khi đã có dữ liệu!
DB_ENCRYPTION_KEY=change-me-to-a-secure-32-byte-key!

# ── Authentication ────────────────────────────────────────────────────────────
# JWT secret key - dùng chuỗi ngẫu nhiên dài ≥ 32 ký tự
JWT_SECRET=change-me-to-a-long-random-secret-key-here

# ── Telegram ─────────────────────────────────────────────────────────────────
TELEGRAM_BOT_TOKEN=your-bot-token-from-botfather
TELEGRAM_CHAT_ID=your-chat-id

# ── Logging ──────────────────────────────────────────────────────────────────
LOG_LEVEL=info   # debug | info | warn | error

# ── Web port ─────────────────────────────────────────────────────────────────
WEB_PORT=8081    # Port truy cập web UI từ host
```

> **Lưu ý bảo mật:** Không commit file `.env` lên git. File này đã được `.gitignore`.

---

### `configs/cameras.yaml`

Cấu hình danh sách camera cho detector. File này được đọc lúc khởi động và có thể reload qua gRPC (hot-reload qua Web UI).

```yaml
cameras:
  - camera_id: cam-01           # ID duy nhất (khớp với detector_camera_id trong DB)
    site_id: site-main          # ID khu vực/tòa nhà
    name: "Camera Nhà kho A"    # Tên hiển thị
    description: "Tầng 1, cửa chính"
    rtsp_url: "${CAM_01_RTSP_URL}"   # Dùng biến môi trường (khuyến nghị)
    # rtsp_url: rtsp://user:pass@ip:554/stream  # Hoặc hardcode
    enabled: true
    codec: h264          # h264 | h265  (quan trọng với camera HEVC/H.265)

    # Tham số inference
    fps_sample: 3        # Số frame/giây lấy mẫu để inference
    imgsz: 640           # Kích thước ảnh đầu vào YOLO (640 hoặc 1280)
    conf_fire: 0.5       # Ngưỡng tin cậy phát hiện lửa
    conf_smoke: 0.4      # Ngưỡng tin cậy phát hiện khói
    conf_other: 0.4      # Ngưỡng tin cậy class 3 (dấu hiệu cháy khác)

    # Tham số quyết định (sliding window)
    window_size: 10      # Số frame trong cửa sổ trượt
    fire_hits: 3         # Số lần phát hiện trong window → kích hoạt alert
    smoke_hits: 3
    cooldown_sec: 60     # Thời gian chờ (giây) giữa 2 alert liên tiếp

  # Thêm camera thứ 2:
  - camera_id: cam-02
    site_id: site-main
    name: "Camera Hành lang B"
    rtsp_url: "${CAM_02_RTSP_URL}"
    enabled: true
    codec: h264
    fps_sample: 3
    imgsz: 640
    conf_fire: 0.5
    conf_smoke: 0.4
    conf_other: 0.4
    window_size: 10
    fire_hits: 3
    smoke_hits: 3
    cooldown_sec: 60
```

> **Ghi chú codec:** Camera HIKVISION/Dahua xuất H.265 → đặt `codec: h265`. Nếu để sai, pipeline sẽ fail.

> **Sau khi thêm camera trong Web UI:** API tự động ghi lại `cameras.yaml` và gửi lệnh reload qua gRPC tới detector — không cần restart container.

---

### `configs/settings.yaml`

Cấu hình hệ thống. Các giá trị nhạy cảm được đọc từ biến môi trường.

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  grpc_port: 50051          # gRPC port của detector
  metrics_port: 9090        # Prometheus metrics

auth:
  jwt_secret: "${JWT_SECRET}"
  jwt_expiry_hours: 24
  bcrypt_cost: 12

inference:
  model_path: "/app/models/best.onnx"
  device: "cuda:0"           # "cuda:0" cho GPU, "cpu" cho CPU-only
  warmup_frames: 3

logging:
  level: "info"              # debug | info | warn | error
  format: "json"             # json | pretty

defaults:                    # Giá trị mặc định cho camera (override trong cameras.yaml)
  fps_sample: 3
  imgsz: 640
  conf_fire: 0.5
  conf_smoke: 0.4
  conf_other: 0.4
  window_size: 10
  fire_hits: 3
  smoke_hits: 3
  cooldown_sec: 60
  reconnect_delay_sec: 5
  max_reconnect_attempts: 10

database:
  host: "${DB_HOST:-localhost}"
  port: 5432
  name: "${DB_NAME:-fire_detect}"
  user: "${DB_USER:-fire_detect}"
  password: "${DB_PASSWORD}"
  max_connections: 10
  encryption_key: "${DB_ENCRYPTION_KEY}"

telegram:
  enabled: true
  bot_token: "${TELEGRAM_BOT_TOKEN}"
  default_chat_id: "${TELEGRAM_CHAT_ID}"
  rate_limit_per_minute: 10

storage:
  snapshot_path: "/app/snapshots"
  retention_days: 30

detector:
  host: "detector"            # Hostname trong Docker network
  grpc_port: 50051
  # stream_port: 51051        # Mặc định = grpc_port + 1000
```

---

## Database Migrations

Migrations tự động chạy khi API container khởi động (`sqlx::migrate!()`). Không cần chạy thủ công khi dùng Docker.

### Chạy thủ công (development)

```bash
# Option 1: Qua Docker exec
docker exec fire-detect-db psql -U fire_detect -d fire_detect \
  -f /docker-entrypoint-initdb.d/0001_initial.sql

# Option 2: Dùng sqlx-cli
export DATABASE_URL=postgres://fire_detect:devpassword@localhost:5432/fire_detect
cargo sqlx migrate run --manifest-path apps/api/Cargo.toml

# Option 3: PowerShell script
cd deploy/docker
./run-migrations-simple.ps1
```

### Danh sách migrations

| File | Nội dung |
|------|---------|
| `0001_initial.sql` | Tạo bảng users, cameras, events; admin user mặc định |
| `0002_detector_camera_id.sql` | Thêm cột `detector_camera_id` (mapping với cameras.yaml) |
| `0003_fix_admin_password.sql` | Sửa password hash admin (bcrypt cost 12) |
| `0004_add_codec_conf_other.sql` | Thêm cột `codec` (h264/h265) và `conf_other` |
| `0005_add_composite_indexes.sql` | Index composite cho query Events/Dashboard |

### Tái tạo SQLx offline cache (khi thêm/sửa query)

```bash
# Start postgres trước
cd deploy/docker
docker compose up -d postgres

# Chờ postgres healthy, sau đó:
cd apps/api
export DATABASE_URL=postgres://fire_detect:devpassword@localhost:5432/fire_detect
cargo sqlx prepare

# .sqlx/ cache được cập nhật → commit vào git
git add .sqlx/
git commit -m "chore: regenerate sqlx offline cache"
```

---

## Development (local)

### API (Rust)

```bash
cd apps/api

# Cần postgres đang chạy
export DATABASE_URL=postgres://fire_detect:devpassword@localhost:5432/fire_detect
export SQLX_OFFLINE=false

# Chạy development
cargo run

# Build release
cargo build --release

# Chạy tests
cargo test

# Check compile (nhanh)
cargo check
```

### Detector (Rust)

```bash
cd apps/detector

# CPU mode
cargo run --no-default-features --features cpu

# GPU mode (cần CUDA + ORT libs)
cargo run --release

# Build release CPU
cargo build --release --no-default-features --features cpu

# Build release GPU
cargo build --release
```

> **Lưu ý:** Detector cần GStreamer dev libraries:
> ```bash
> # Ubuntu/Debian
> sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
>   libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-ugly gstreamer1.0-libav
> ```

### Web (React)

```bash
cd apps/web

# Cài dependencies
npm install

# Dev server (hot reload, proxy tới API localhost:8080)
npm run dev
# → http://localhost:5173

# Build production
npm run build

# Preview build
npm run preview

# Lint
npm run lint

# Mock API (test UI không cần backend)
npm run mock-api
# → http://localhost:3001 (mock server)
# → npm run dev (dùng mock)
```

> Dev server tự động proxy `/api/` và `/ws/` tới `http://localhost:8080`.
> Sửa `vite.config.ts` nếu cần đổi địa chỉ API.

---

## API Endpoints

### Authentication

| Method | Endpoint | Body | Mô tả |
|--------|----------|------|-------|
| `POST` | `/api/auth/login` | `{"email":"...","password":"..."}` | Đăng nhập, trả về JWT token |

### Cameras

| Method | Endpoint | Mô tả |
|--------|----------|-------|
| `GET` | `/api/cameras` | Danh sách tất cả camera |
| `POST` | `/api/cameras` | Thêm camera mới |
| `GET` | `/api/cameras/:id` | Chi tiết một camera |
| `PUT` | `/api/cameras/:id` | Cập nhật camera |
| `DELETE` | `/api/cameras/:id` | Xóa camera |
| `GET` | `/api/cameras/:id/stream/mjpeg` | **Live MJPEG stream** (auth qua header hoặc `?token=`) |

### Events

| Method | Endpoint | Query Params | Mô tả |
|--------|----------|-------------|-------|
| `GET` | `/api/events` | `page`, `page_size`, `camera_id`, `event_type`, `start`, `end` | Danh sách sự kiện (có phân trang) |
| `GET` | `/api/events/count` | `camera_id`, `event_type`, `start`, `end` | Đếm tổng sự kiện |
| `POST` | `/api/events/:id/acknowledge` | — | Xác nhận đã xử lý sự kiện |

### Snapshots

| Method | Endpoint | Mô tả |
|--------|----------|-------|
| `GET` | `/api/snapshots/:camera_id/:filename` | Tải ảnh snapshot |

### Settings

| Method | Endpoint | Mô tả |
|--------|----------|-------|
| `GET` | `/api/settings/telegram` | Xem cấu hình Telegram (token bị mask) |
| `PUT` | `/api/settings/telegram` | Cập nhật cấu hình Telegram |
| `POST` | `/api/settings/telegram/test` | Gửi tin nhắn Telegram test |

### Health

| Method | Endpoint | Mô tả |
|--------|----------|-------|
| `GET` | `/health` | Health check (không cần auth) |

### WebSocket

| Endpoint | Mô tả |
|----------|-------|
| `WS /ws/events` | Stream sự kiện realtime (fire/smoke alert, stream_up/down) |

---

### Ví dụ sử dụng API

```bash
# Login
TOKEN=$(curl -s -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"admin123"}' \
  | jq -r '.token')

# Danh sách camera
curl http://localhost:8080/api/cameras \
  -H "Authorization: Bearer $TOKEN" | jq

# Xem events (trang 1, 20 bản ghi)
curl "http://localhost:8080/api/events?page=1&page_size=20" \
  -H "Authorization: Bearer $TOKEN" | jq

# Stream MJPEG (trong browser hoặc VLC)
# URL: http://localhost:8080/api/cameras/{camera-uuid}/stream/mjpeg?token={jwt}

# Health check
curl http://localhost:8080/health
```

---

## Luồng dữ liệu & Stream

### Detector → API (gRPC events)

```
Detector phát hiện fire/smoke
  → Decision engine (sliding window hits >= threshold)
  → Lưu snapshot JPG vào /app/snapshots/{camera_id}/{timestamp}.jpg
  → Gửi gRPC event tới API
  → API lưu event vào PostgreSQL
  → API gửi Telegram alert (nếu enabled)
  → API broadcast qua WebSocket tới tất cả browser đang kết nối
```

### Live Stream MJPEG

```
Browser → GET /api/cameras/{id}/stream/mjpeg?token={jwt}
  → nginx (proxy_buffering off, proxy_read_timeout 3600s)
  → API proxy → GET http://detector:51051/stream/{detector_camera_id}/mjpeg
  → Detector BroadcastStream → encode JPEG (quality 75, ~2-5ms/frame)
  → multipart/x-mixed-replace boundary=frame
  → Browser hiển thị trong CameraStreamModal
```

**Latency:** ~200ms (vs ~6s với HLS)

### Camera Hot-Reload

```
Web UI thêm/sửa/xóa camera
  → API CRUD (cameras table, RTSP URL encrypted AES-256-GCM)
  → camera_sync.rs ghi cameras.yaml từ DB
  → gRPC reload_config → detector.reload_cameras()
  → Worker cũ stop, worker mới start — không restart container
```

---

## Monitoring

### Prometheus + Grafana

```bash
# Chạy stack monitoring (thêm vào compose nếu cần)
# Detector expose metrics tại :9090/metrics

# Kiểm tra metrics
curl http://localhost:9090/metrics
```

Dashboard Grafana có sẵn tại: `deploy/grafana/dashboards/fire-detection.json`

### Logs

```bash
# Xem logs realtime tất cả services
cd deploy/docker
docker compose logs -f

# Logs detector với filter
docker compose logs detector -f | grep -E "fire|smoke|ERROR|WARN"

# Logs API
docker compose logs api -f

# Export logs ra file
docker compose logs detector > /tmp/detector.log 2>&1
```

---

## Troubleshooting

### Detector không kết nối được camera RTSP

```bash
# Kiểm tra log detector
docker compose logs detector --tail=50

# Test RTSP từ host (dùng ffprobe)
ffprobe rtsp://user:pass@camera-ip:554/stream

# Test từ trong container
docker exec -it fire-detect-detector bash
gst-launch-1.0 rtspsrc location="rtsp://user:pass@ip:554/stream" ! fakesink
```

**Các lỗi thường gặp:**
- `Pipeline error`: Sai `codec` trong cameras.yaml (camera H.265 nhưng đặt h264)
- `Connection refused`: Camera không accessible từ Docker network → kiểm tra IP/port
- `Authentication failed`: Sai username/password trong RTSP URL

### API không kết nối được database

```bash
docker compose logs api --tail=20
# Tìm: "Database connected" hoặc lỗi connection

# Kiểm tra postgres
docker compose exec postgres pg_isready -U fire_detect

# Kết nối thủ công
docker exec -it fire-detect-db psql -U fire_detect -d fire_detect
```

### Stream MJPEG không load

```bash
# Kiểm tra detector có đang stream không
curl -I http://localhost:51051/stream/cam-01/mjpeg 2>&1
# → 404 nếu camera_id sai

# Kiểm tra detector_camera_id trong DB
docker exec fire-detect-db psql -U fire_detect -d fire_detect \
  -c "SELECT id, name, detector_camera_id FROM cameras;"

# Giá trị detector_camera_id phải khớp với camera_id trong cameras.yaml
```

### GPU không được nhận diện

```bash
# Kiểm tra NVIDIA runtime
docker run --rm --gpus all nvidia/cuda:12.6.3-base-ubuntu24.04 nvidia-smi

# Nếu lỗi: cài NVIDIA Container Toolkit
# https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html

# Kiểm tra logs detector
docker compose logs detector | grep -E "cuda|CUDA|gpu|GPU|ort"
```

### Build API thất bại (SQLx offline cache)

```bash
# Tái tạo cache
cd deploy/docker
docker compose up -d postgres   # Chạy postgres

# Đợi healthy
sleep 5

# Regenerate
cd ../../apps/api
DATABASE_URL=postgres://fire_detect:devpassword@localhost:5432/fire_detect \
  cargo sqlx prepare

# Rebuild
cd ../../deploy/docker
docker build -f Dockerfile.api -t fire-detect-api:latest ../../
```

### Reset toàn bộ (xóa data)

```bash
cd deploy/docker
docker compose down -v          # Xóa containers + volumes
docker rmi fire-detect-api fire-detect-detector fire-detect-web  # Xóa images
docker compose up -d --build    # Build lại từ đầu
```

---

## Tài khoản mặc định

| Thông tin | Giá trị |
|-----------|---------|
| Email | `admin@example.com` |
| Password | `admin123` |
| Role | `admin` |

> **Đổi password ngay sau lần đăng nhập đầu tiên** (chưa có endpoint đổi password → chạy SQL thủ công):
> ```bash
> # Tạo bcrypt hash mới (cost 12) bằng Python
> python3 -c "import bcrypt; print(bcrypt.hashpw(b'new-password', bcrypt.gensalt(12)).decode())"
>
> # Cập nhật DB
> docker exec fire-detect-db psql -U fire_detect -d fire_detect \
>   -c "UPDATE users SET password_hash = '\$2b\$12\$...' WHERE email = 'admin@example.com';"
> ```

---

## Truy cập sau khi chạy

| Service | URL |
|---------|-----|
| Web UI | http://localhost:8081 |
| API | http://localhost:8080 |
| Health check | http://localhost:8080/health |
| Prometheus metrics | http://localhost:9090/metrics |

---

## License

MIT License — MobifoneSolutions © 2025
