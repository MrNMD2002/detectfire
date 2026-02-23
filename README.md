# Fire & Smoke Detection System

Hệ thống phát hiện cháy và khói bằng AI, sử dụng **YOLOv26** (SalahALHaismawi/yolov26-fire-detection) với real-time inference trên GPU/CPU.

📄 **Cấu trúc project đầy đủ:** xem [docs/PROJECT-STRUCTURE.md](docs/PROJECT-STRUCTURE.md) (dùng để gửi đồng nghiệp).

## Tính năng

- 🔥 **Phát hiện cháy và khói** - Real-time inference với YOLOv26 (fire, smoke, other)
- 📹 **Multi-camera RTSP** - Hỗ trợ nhiều camera với reconnection tự động
- 🚀 **GPU Inference** - ONNX Runtime với CUDA/TensorRT
- 📱 **Telegram Alerts** - Thông báo tức thì với ảnh snapshot
- 🖥️ **Web Dashboard** - Giao diện React hiện đại
- 📊 **Monitoring** - Grafana/Loki/Prometheus integration
- 🔐 **Security** - JWT Auth, RTSP URL encryption

## Kiến trúc

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  RTSP Cameras   │────▶│    Detector     │────▶│   PostgreSQL    │
│                 │     │ (Rust+GStreamer+ONNX)│  │                 │
└─────────────────┘     └────────┬────────┘     └─────────────────┘
                                 │
                                 │ gRPC
                                 ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Web Browser   │◀───▶│   API Service   │────▶│    Telegram     │
│   (React SPA)   │     │  (Rust+Axum)    │     │      Bot        │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

## Yêu cầu

- **Hardware**: NVIDIA GPU (RTX 3060 recommended)
- **Software**: Docker, Docker Compose, NVIDIA Container Toolkit
- **OS**: Ubuntu 22.04 / Windows 11 with WSL2

## Quick Start

### 1. Clone repository

```bash
git clone https://github.com/your-org/fire-detect.git
cd fire-detect
```

### 2. Configure environment

```bash
cp deploy/docker/.env.example deploy/docker/.env
# Edit .env with your settings
```

### 3. Start services

```bash
cd deploy/docker
docker compose up -d
```

### 4. Access dashboard

- Web UI: http://localhost
- API: http://localhost:8080
- Default login: admin@example.com / admin123

## Configuration

### cameras.yaml

```yaml
defaults:
  fps_sample: 3
  imgsz: 640
  conf_fire: 0.5
  conf_smoke: 0.4
  window_size: 10
  fire_hits: 3
  smoke_hits: 3
  cooldown_sec: 60

cameras:
  - camera_id: cam-a01
    site_id: site-a
    name: "Camera Nhà kho A"
    rtsp_url: "${RTSP_CAM_A01_URL}"
```

### settings.yaml

```yaml
server:
  api:
    host: "0.0.0.0"
    port: 8080
  detector:
    grpc_port: 50051

telegram:
  enabled: true
  bot_token: "${TELEGRAM_BOT_TOKEN}"
  default_chat_id: "${TELEGRAM_CHAT_ID}"
```

## Development

### Build detector

```bash
cd apps/detector
cargo build --release
```

### Build API

```bash
cd apps/api
cargo build --release
```

### Run web dev

```bash
cd apps/web
npm install
npm run dev
```

## API Endpoints

| Method | Endpoint                    | Description       |
| ------ | --------------------------- | ----------------- |
| POST   | /api/auth/login             | Login             |
| GET    | /api/cameras                | List cameras      |
| POST   | /api/cameras                | Add camera        |
| GET    | /api/events                 | List events       |
| POST   | /api/events/:id/acknowledge | Acknowledge event |
| WS     | /ws/events                  | Real-time events  |

## License

MIT License - MobifoneSolutions © 2024
