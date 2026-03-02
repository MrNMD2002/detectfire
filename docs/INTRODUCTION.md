# Hệ Thống Phát Hiện Cháy & Khói Thời Gian Thực
### Fire & Smoke Detection System — Giới thiệu kỹ thuật

> **Phát triển bởi:** MobifoneSolutions
> **Phiên bản:** 0.1.0
> **Cập nhật:** 2026-03-02

---

## Tổng quan

Hệ thống phát hiện cháy và khói tự động từ camera giám sát RTSP, sử dụng mô hình AI (YOLOv10) chạy trên GPU. Khi phát hiện sự cố, hệ thống ngay lập tức gửi cảnh báo qua Telegram kèm ảnh chụp có đánh dấu vị trí phát hiện. Toàn bộ stack được containerize bằng Docker, chạy ổn định trên môi trường production.

```
Camera RTSP → Detector (AI/GPU) → API → Web Dashboard + Telegram
```

---

## Kiến trúc tổng thể

```
┌─────────────────────────────────────────────────────────────────────┐
│                         DOCKER NETWORK                              │
│                                                                     │
│  ┌──────────┐    RTSP     ┌─────────────────────────────────────┐  │
│  │ Camera   │ ──────────▶ │         DETECTOR SERVICE            │  │
│  │ (H.264/  │             │         (Rust + GPU)                │  │
│  │  H.265)  │             │                                     │  │
│  └──────────┘             │  GStreamer → Inference → Decision   │  │
│                           │      │            │           │     │  │
│                           │  Frame Push    YOLO AI    Sliding   │  │
│                           │  (broadcast)  TensorRT    Window   │  │
│                           │      │                       │     │  │
│                           │  MJPEG Stream          Event Pub   │  │
│                           └──────┬────────────────────┬────────┘  │
│                                  │ gRPC :50051         │ HTTP POST │
│                    MJPEG stream  │                     ▼           │
│  ┌──────────┐ ◀────────────────  │         ┌──────────────────┐  │
│  │   WEB    │                    │         │   API SERVICE    │  │
│  │  (React) │ ◀── REST :8080 ───▶│         │  (Rust + Axum)   │  │
│  │  :8081   │                    │         │                  │  │
│  └──────────┘                    │         │  REST API        │  │
│                                  │         │  WebSocket       │  │
│                                  └────────▶│  JWT Auth        │  │
│                                            │  Migrations      │  │
│                                            └────────┬─────────┘  │
│                                                     │            │
│                                            ┌────────▼─────────┐  │
│                                            │    PostgreSQL 16  │  │
│                                            │   (Events, Cam,  │  │
│                                            │    Settings)     │  │
│                                            └──────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                         │ Telegram Bot API
                         ▼
               📱 Telegram Notification
                  (ảnh + bounding box)
```

---

## Pipeline xử lý chi tiết

### Bước 1 — Ingest RTSP (GStreamer)

```
rtspsrc (RTSP/RTP)
  └─▶ [pad_added callback]
        ├── H.264: rtph264depay → h264parse → avdec_h264
        └── H.265: rtph265depay → h265parse → avdec_h265
              └─▶ queue → videoconvert → videoscale
                    └─▶ capsfilter (video/x-raw, format=RGB, width=640, height=640)
                          └─▶ appsink [set_callbacks — PUSH MODEL]
                                └─▶ broadcast::Sender<Arc<Frame>>
```

**Tại sao dùng Push model thay Pull?**
- Pull (`try_pull_sample` vòng lặp): tốn CPU liên tục, dễ miss frame khi GPU bận
- Push (`set_callbacks`): GStreamer gọi callback mỗi khi có frame mới, zero-copy qua `Arc<Vec<u8>>`
- Nhiều consumer cùng nhận một frame (inference worker + MJPEG server) không cần copy bộ nhớ

---

### Bước 2 — Frame Sampling & Queue

```
broadcast::Receiver<Arc<Frame>>  (trong CameraWorker)
  └─▶ fps_sample: 3  →  lấy 1 frame / 3 giây  (giảm tải GPU)
        └─▶ flume::bounded(1)  →  inference queue (drop frame cũ nếu GPU còn bận)
```

**Tại sao dùng `flume::bounded(1)`?**
Nếu inference chậm hơn frame rate, queue không được phép tích lũy frame cũ — detection phải luôn trên frame **mới nhất**, không phải frame 5 giây trước.

---

### Bước 3 — Preprocessing

```
Arc<Frame> (RGB, 640×640, u8)
  └─▶ preprocess_frame()
        ├── Normalize: pixel / 255.0  →  [0.0, 1.0] f32
        ├── HWC → CHW: (H,W,C) → (C,H,W)
        └─▶ ndarray::Array4<f32>  shape [1, 3, 640, 640]
```

---

### Bước 4 — AI Inference (ONNX Runtime + TensorRT)

```
Array4<f32> [1, 3, 640, 640]
  └─▶ ORT Session::run()
        ├── TensorRT EP (FP16 Tensor Cores)  ← ưu tiên 1
        │     └── Compile engine lần đầu (~2-3 phút), cache vào volume
        └── CUDA EP (fallback nếu TRT không available)
              └─▶ Vec<f32>  (raw model output)
```

**Model:** YOLOv10/YOLOv26 ONNX — 3 class:
| Class ID | Nhãn | Màu bbox |
|---|---|---|
| 0 | fire (lửa) | 🔴 Đỏ |
| 1 | other | — (bỏ qua) |
| 2 | smoke (khói) | 🟠 Cam |

---

### Bước 5 — Postprocessing (NMS)

```
Vec<f32> (raw output)
  ├── End-to-end NMS model (output shape: N×6):
  │     [x1, y1, x2, y2, confidence, class_id]  — pixel coords [0, 640]
  │     └─▶ postprocess(): lọc theo conf threshold, scale về ảnh gốc
  │
  └── Standard YOLO (output shape: 7×N nếu >10000 phần tử):
        └─▶ postprocess_with_probs(): NMS thủ công + IoU threshold 0.45
```

**Bug đã fix:** Bounding box gửi Telegram trống vì code cũ `clamp(0,1)` trên giá trị pixel 100.0 → 1.0 → box collapse. Fix: `(pixel / 640.0).clamp(0.0, 1.0)`.

---

### Bước 6 — Decision Engine (Sliding Window)

```
Vec<Detection>  (mỗi inference)
  └─▶ SlidingWindow (per camera, size=10 frames)
        ├── Đếm số frame có fire  ≥ fire_hits (3)  → fire_alert
        └── Đếm số frame có smoke ≥ smoke_hits (3) → smoke_alert
              └─▶ CooldownManager: nếu vừa alert → chờ cooldown_sec (60s) trước khi alert tiếp
```

**Tại sao dùng Sliding Window thay threshold đơn?**
Một frame có thể false positive (phản sáng, bóng tối). Cần ít nhất 3/10 frame liên tiếp xác nhận mới trigger alert — giảm báo động giả đáng kể.

---

### Bước 7 — Event Publishing

```
DecisionEvent (fire/smoke)
  └─▶ EventPublisher
        ├── 1. Chụp snapshot frame hiện tại
        │     └── draw_bounding_boxes() → JPEG bytes
        ├── 2. POST /api/events  →  API lưu vào PostgreSQL
        │     └── Snapshot bytes upload kèm
        └── 3. Telegram Bot API
              └── sendPhoto (ảnh có bbox + caption)
```

---

### Bước 8 — Live Stream (MJPEG)

```
broadcast::Receiver<Arc<Frame>>  (subscriber từ CameraManager)
  └─▶ MJPEG Server (axum, port 3030 internal)
        └── JpegEncoder::new_with_quality(75)
              └── multipart/x-mixed-replace stream
                    └─▶ API proxy  GET /stream/{camera_id}
                          └─▶ Web CameraStreamModal (JS JPEG parser)
                                └── <img src={blobUrl}> — cập nhật mỗi frame (~200ms)
```

---

## Công nghệ & Framework sử dụng

### Backend — Detector Service

| Công nghệ | Version | Vai trò |
|---|---|---|
| **Rust** | 1.75+ (stable) | Ngôn ngữ chính — zero-cost abstractions, memory safety without GC |
| **Tokio** | 1.35 | Async runtime — xử lý đồng thời nhiều camera |
| **GStreamer** | 0.22 (gst-rs) | RTSP ingest, decode H.264/H.265 |
| **ONNX Runtime** | 1.23.2 | Inference engine — chạy model YOLO |
| **TensorRT EP** | 10.x | GPU optimization — FP16 Tensor Cores |
| **CUDA** | 12.6 | GPU compute |
| **ndarray** | 0.15 | Tensor manipulation (preprocessing CHW) |
| **image + imageproc** | 0.25 / 0.24 | Vẽ bounding box lên snapshot |
| **ab_glyph** | 0.2 | Render text label trên ảnh |
| **parking_lot** | 0.12 | RwLock/Mutex hiệu suất cao |
| **tonic + prost** | 0.11 / 0.12 | gRPC server (hot reload config) |
| **axum** | 0.7 | HTTP server nội bộ (MJPEG stream) |
| **tracing** | 0.1 | Structured logging (JSON) |
| **metrics-exporter-prometheus** | 0.13 | Expose metrics cho Prometheus |

### Backend — API Service

| Công nghệ | Version | Vai trò |
|---|---|---|
| **Rust** | 1.75+ | Ngôn ngữ chính |
| **Axum** | 0.7 | Web framework (REST API) |
| **SQLx** | 0.7 | Async PostgreSQL client, compile-time query check |
| **PostgreSQL** | 16 | Database — events, cameras, settings |
| **JWT** | jsonwebtoken | Xác thực người dùng |
| **bcrypt** | 12 rounds | Hash mật khẩu |
| **AES-256-GCM** | — | Mã hóa RTSP URL trong DB |
| **WebSocket** | tokio-tungstenite | Push event real-time về Web |
| **reqwest** | — | HTTP client gọi Telegram Bot API |
| **tower-http** | 0.5 | CORS, compression, tracing middleware |

### Frontend — Web Dashboard

| Công nghệ | Version | Vai trò |
|---|---|---|
| **React** | 18 | UI framework |
| **TypeScript** | 5 | Type safety |
| **Vite** | 5 | Build tool, dev server |
| **TanStack Query** | 5 | Server state management, cache, refetch |
| **React Router** | 6 | Client-side routing |
| **Lucide React** | — | Icon library |
| **Nginx** | alpine | Serve static files, proxy API |

### Infrastructure

| Công nghệ | Vai trò |
|---|---|
| **Docker** | Container runtime |
| **Docker Compose** | Orchestration multi-service |
| **NVIDIA Container Toolkit** | GPU passthrough vào container |
| **Prometheus** | Thu thập metrics |
| **Grafana** | Dashboard monitoring |
| **Loki + Promtail** | Log aggregation |

---

## System Design — Quyết định thiết kế

### Tại sao chọn Rust thay Python?

| Tiêu chí | Python | Rust |
|---|---|---|
| **Hiệu năng** | GIL giới hạn đa luồng thật sự | True parallelism, zero-cost abstractions |
| **Memory** | GC overhead, ~200-500MB cho CV app | ~30-50MB footprint, không có GC pause |
| **Latency** | GC pause không dự đoán được | Deterministic latency |
| **RTSP + GStreamer** | Python bindings kém ổn định | gst-rs bindings type-safe, production-grade |
| **Concurrency** | asyncio + threading phức tạp | Tokio async, Rust ownership đảm bảo thread safety |
| **Deployment** | Cần Python runtime, nhiều dependency | Single binary, Docker image nhỏ hơn |
| **Compile-time safety** | Runtime errors | Compile-time: null safety, memory safety, data race free |

### Tại sao TensorRT thay vì CUDA EP thuần?

| | CUDA EP (FP32) | TensorRT EP (FP16) |
|---|---|---|
| Inference/frame | ~100-150ms | **~47ms** |
| Throughput | ~7 FPS | **~21 FPS** |
| GPU Memory | Cao hơn (FP32) | Thấp hơn ~50% (FP16) |
| First run | Ngay lập tức | Compile engine ~2-3 phút |
| Subsequent runs | — | Load cache <5 giây |

TRT compile ONNX model thành engine tối ưu cho GPU cụ thể, dùng FP16 Tensor Cores — nhanh hơn 3x vs CUDA EP FP32.

### Tại sao MJPEG thay HLS?

| | HLS | MJPEG |
|---|---|---|
| Latency | 6-10 giây | **~200ms** |
| Complexity | Cần segment .ts + playlist | Một HTTP response stream |
| Client | Cần HLS.js library | Native fetch + JS parser |
| Firewall | OK | OK |
| Bandwidth | Tốt hơn (compressed segment) | Cao hơn một chút |

Với hệ thống giám sát cháy, 6-10 giây là không chấp nhận được. MJPEG cho phép phản ứng gần real-time.

### Tại sao Sliding Window Decision Engine?

Một frame đơn lẻ có thể false positive (phản sáng đèn flash, mặt trời chiếu, bóng tối đột ngột). Sliding window yêu cầu phát hiện nhất quán qua nhiều frame liên tiếp:

```
Window size = 10 frames (tương đương ~3.3 giây ở 3 FPS)
Fire alert   = cần ≥ 3 frame trong window có lửa
Smoke alert  = cần ≥ 3 frame trong window có khói
Cooldown     = 60 giây sau mỗi alert (tránh spam)
```

---

## Cấu hình hệ thống hiện tại

```yaml
# configs/cameras.yaml
cameras:
  - camera_id:    cam-01
    name:         Camera Công Ty
    codec:        h265          # H.265/HEVC
    fps_sample:   3             # lấy mẫu 3 FPS
    imgsz:        640           # input model 640×640
    conf_fire:    0.57          # ngưỡng confidence lửa
    conf_smoke:   0.50          # ngưỡng confidence khói
    conf_other:   0.45
    window_size:  10            # sliding window 10 frames
    fire_hits:    3             # cần 3 hit để alert
    smoke_hits:   3
    cooldown_sec: 60            # cooldown 60s giữa các alert
```

---

## Ports & Endpoints

| Service | Port | Mô tả |
|---|---|---|
| Web Dashboard | `8081` | Giao diện người dùng |
| API REST | `8080` | REST API, WebSocket |
| Detector gRPC | `50051` | Internal — hot reload config |
| Detector MJPEG | `3030` | Internal — live stream |
| PostgreSQL | `5432` | Database |
| Prometheus | `9090` | Metrics scrape |
| Grafana | `3000` | Monitoring dashboard |
| Loki | `3100` | Log aggregation |

### API chính

```
POST   /api/auth/login              — Đăng nhập, nhận JWT
GET    /api/cameras                 — Danh sách camera
POST   /api/cameras                 — Thêm camera (hot reload)
PUT    /api/cameras/:id             — Sửa camera
DELETE /api/cameras/:id             — Xóa camera
GET    /api/events?page=&limit=     — Danh sách sự kiện (có pagination)
GET    /api/events/count            — Đếm sự kiện theo bộ lọc
GET    /api/snapshots/:cam/:file    — Ảnh snapshot
GET    /stream/:camera_id           — Live MJPEG stream
GET    /api/settings/telegram       — Xem cấu hình Telegram (masked)
POST   /api/settings/telegram       — Cập nhật cấu hình Telegram
POST   /api/settings/telegram/test  — Gửi tin nhắn test
WS     /ws                          — WebSocket real-time events
```

---

## Bảo mật

| Vấn đề | Giải pháp |
|---|---|
| RTSP URL (chứa password camera) | Mã hóa **AES-256-GCM** trong PostgreSQL, chỉ decrypt khi cần |
| API authentication | **JWT** (HS256), expiry 24h |
| Password lưu DB | **bcrypt** cost 12 |
| Telegram token | Biến môi trường (không hardcode), masked khi GET API |
| CORS | Chỉ cho phép origin từ web service |

---

## Monitoring

```
Detector/API service
  └─▶ /metrics  (Prometheus format)
        └─▶ Prometheus scrape mỗi 15s
              └─▶ Grafana dashboard
                    ├── Inference latency (histogram)
                    ├── Events per minute
                    ├── Camera stream uptime
                    └── GPU memory usage

Docker logs (stdout/stderr JSON)
  └─▶ Promtail collect
        └─▶ Loki store
              └─▶ Grafana Explore (query log)
```

---

## Deployment

```bash
# Yêu cầu:
# - Docker + Docker Compose
# - NVIDIA GPU + nvidia-container-toolkit
# - Model file: models/best.onnx

# Khởi động toàn bộ stack
docker compose up -d

# Kèm monitoring (Prometheus + Grafana + Loki)
docker compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d

# Xem log detector
docker logs fire-detect-detector -f

# Hot reload camera config (không cần restart)
# → Thay đổi qua Web UI tại http://localhost:8081
```

---

## Hiệu năng đo được

| Metric | Giá trị |
|---|---|
| Inference latency (TRT FP16) | **~47ms/frame** avg |
| Live stream latency (MJPEG) | **~150-200ms** |
| CPU usage (inference) | <5% (GPU xử lý) |
| GPU utilization @ 3FPS/camera | ~15% |
| Số camera tối đa (ước tính) | **6-7 camera** trên 1 GPU |
| Alert false positive rate | Thấp (Sliding Window 3/10 frame) |
| Telegram notification delay | <2 giây từ khi phát hiện |
