# Cấu trúc project – Fire & Smoke Detection System

Tài liệu mô tả đầy đủ cấu trúc repo để đồng nghiệp clone, build và chạy hệ thống.

---

## 1. Tổng quan

- **Mục đích:** Hệ thống phát hiện cháy/khói real-time từ camera RTSP, dùng YOLOv26 (ONNX), có API, Web UI, Telegram.
- **Stack:** Rust (detector + API), React/TypeScript (web), PostgreSQL, Docker.
- **Model:** YOLOv26 fire detection (SalahALHaismawi/yolov26-fire-detection), 3 class: fire, other, smoke.

---

## 2. Cây thư mục đầy đủ

```text
Detect_Fire_and_smoke/
├── .claude/                          # Cấu hình AI agent (Claude)
│   └── agents/
│       └── fullstack-developer.md
├── .dockerignore                     # Bỏ qua khi build Docker image
├── .gitignore
├── LICENSE
├── README.md                         # Hướng dẫn nhanh, Quick Start
├── run_inference.py                  # Script Python chạy inference (YOLO .pt) – dev/test
│
├── apps/
│   ├── api/                          # REST API (Rust, Axum)
│   │   ├── build.rs                  # Compile proto gRPC client
│   │   ├── Cargo.toml
│   │   ├── Cargo.lock
│   │   ├── migrations/
│   │   │   ├── 0001_initial.sql      # users, cameras, events
│   │   │   └── 0002_detector_camera_id.sql
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── db.rs
│   │       ├── auth.rs
│   │       ├── telegram.rs
│   │       ├── ws.rs
│   │       ├── detector_client.rs    # gRPC client tới detector
│   │       ├── error.rs
│   │       ├── models.rs
│   │       └── routes/
│   │           ├── mod.rs
│   │           ├── auth.rs
│   │           ├── cameras.rs
│   │           ├── events.rs
│   │           ├── health.rs
│   │           └── stream.rs
│   │
│   ├── detector/                     # Service inference + RTSP (Rust, GStreamer, ONNX)
│   │   ├── build.rs                  # Compile proto gRPC
│   │   ├── Cargo.toml
│   │   ├── Cargo.lock
│   │   ├── proto/
│   │   │   └── detector.proto
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config/               # Load/validate config
│   │       │   ├── mod.rs
│   │       │   ├── loader.rs
│   │       │   ├── models.rs
│   │       │   └── validation.rs
│   │       ├── camera/               # RTSP ingest, sampler, pipeline
│   │       │   ├── mod.rs
│   │       │   ├── manager.rs
│   │       │   ├── pipeline.rs
│   │       │   ├── sampler.rs
│   │       │   ├── status.rs
│   │       │   └── worker.rs
│   │       ├── inference/            # ONNX Runtime, pre/post process
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs
│   │       │   ├── detection.rs
│   │       │   ├── preprocess.rs
│   │       │   └── postprocess.rs
│   │       ├── decision/             # Sliding window, cooldown
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs
│   │       │   ├── window.rs
│   │       │   └── cooldown.rs
│   │       ├── event/                # Publish event, gRPC server
│   │       │   ├── mod.rs
│   │       │   ├── publisher.rs
│   │       │   ├── detector_grpc.rs
│   │       │   └── models.rs
│   │       ├── stream/               # HLS stream HTTP server
│   │       │   ├── mod.rs
│   │       │   └── server.rs
│   │       ├── error.rs
│   │       └── metrics.rs
│   │
│   └── web/                          # Frontend React (Vite, TypeScript)
│       ├── package.json
│       ├── package-lock.json
│       ├── index.html
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── tsconfig.node.json
│       ├── mock-api.js / mock-api.mjs
│       ├── README.md
│       └── src/
│           ├── main.tsx
│           ├── App.tsx
│           ├── lib/
│           │   └── api.ts
│           ├── hooks/
│           │   └── useWebSocket.ts
│           ├── layouts/
│           │   └── MainLayout.tsx
│           ├── pages/
│           │   ├── LoginPage.tsx
│           │   ├── DashboardPage.tsx
│           │   ├── CamerasPage.tsx
│           │   ├── EventsPage.tsx
│           │   └── SettingsPage.tsx
│           ├── components/
│           │   └── CameraStreamModal.tsx
│           ├── stores/
│           │   └── authStore.ts
│           └── styles/
│               └── index.css
│
├── configs/                          # Cấu hình chạy (mount vào Docker)
│   ├── settings.yaml                # Server, inference, DB, Telegram, storage
│   └── cameras.yaml                 # Danh sách camera RTSP + defaults
│
├── models/                           # Model YOLO + script export ONNX
│   ├── README.md                    # Mô tả model, cách export
│   ├── requirements.txt             # Python deps cho export
│   ├── export_onnx.py              # best.pt → best.onnx
│   └── export_and_validate.ps1     # PowerShell: export + validate
│   # best.pt / best.onnx không commit (gitignore), cần tự export hoặc copy
│
├── deploy/
│   ├── docker/                      # Docker build & run
│   │   ├── .env.example             # Mẫu biến môi trường
│   │   ├── docker-compose.yml      # Mặc định: CPU
│   │   ├── docker-compose.prod-cpu.yml
│   │   ├── docker-compose.prod-gpu.yml
│   │   ├── docker-compose.dev-infra.yml
│   │   ├── Dockerfile.api
│   │   ├── Dockerfile.web
│   │   ├── Dockerfile.detector     # Detector GPU (CUDA)
│   │   ├── Dockerfile.detector.cpu
│   │   ├── nginx.conf               # Reverse proxy cho web + API
│   │   ├── build-and-run.ps1       # Script build + start (Windows)
│   │   ├── init-db.sh
│   │   ├── run-migrations.ps1 / run-migrations-simple.ps1
│   │   ├── verify-migration.sh
│   │   ├── build-api.ps1 / build-commands.ps1
│   │   ├── README.md
│   │   ├── QUICK-START.md
│   │   ├── BUILD.md
│   │   └── DEBUG-DETECTOR.md       # Gỡ lỗi detector restart loop
│   ├── grafana/                     # Monitoring (tùy chọn)
│   │   ├── dashboards/
│   │   │   └── fire-detection.json
│   │   └── provisioning/
│   │       ├── dashboards/
│   │       └── datasources/
│   └── promtail.yml
│
├── docs/                             # Tài liệu kỹ thuật
│   ├── PROJECT-STRUCTURE.md         # File này – cấu trúc repo
│   ├── architecture.md
│   ├── api-spec.md
│   └── runbook.md
│
├── scripts/
│   └── test-system.ps1
│
└── MBFS_Stream/                      # Thư viện/plugin GStreamer (nội bộ, tùy chọn)
    └── mbfs-stream/
        ├── Cargo.toml
        ├── src/
        ├── examples/
        └── docs/
```

---

## 3. Mô tả từng phần chính

### 3.1. `apps/api` (REST API)

- **Công việc:** Đăng nhập (JWT), CRUD camera, danh sách event, acknowledge, WebSocket event real-time, gọi detector qua gRPC, gửi Telegram.
- **DB:** PostgreSQL, schema trong `migrations/`, dùng SQLx (compile-time check).
- **Chạy:** Cần `CONFIG_DIR`, `DATABASE_URL`; build API cần DB đang chạy (để sqlx verify).

### 3.2. `apps/detector` (Detector service)

- **Công việc:** Đọc RTSP (GStreamer), lấy frame → inference ONNX (YOLOv26), sliding window + cooldown → gửi event qua gRPC cho API; phục vụ HLS stream.
- **Model:** Đọc từ `configs` (trỏ tới `/app/models/best.onnx` trong Docker). Class: 0=fire, 1=other, 2=smoke.
- **Build:** Cần Rust, GStreamer dev libs; feature `gpu` (ort/cuda, tensorrt) hoặc `cpu` (ort/load-dynamic).

### 3.3. `apps/web` (Frontend)

- **Công việc:** Login, dashboard, danh sách camera/event, xem stream HLS, cấu hình.
- **Chạy:** `npm install` + `npm run dev` (dev) hoặc build tĩnh, Docker dùng nginx serve `dist/` và proxy `/api`, `/ws` sang API.

### 3.4. `configs/`

- **settings.yaml:** Cổng, đường dẫn model, device (cuda:0/cpu), Telegram, DB, storage, logging.
- **cameras.yaml:** Danh sách camera (camera_id, site_id, rtsp_url, enabled, conf_fire/conf_smoke/conf_other, …). Detector và API đều đọc qua `CONFIG_DIR`.

### 3.5. `models/`

- **best.pt:** Weights YOLO (không commit). Export từ Hugging Face hoặc train riêng.
- **best.onnx:** Detector Docker chỉ dùng file này. Tạo bằng:
  - `python models/export_onnx.py --weights models/best.pt --output models/best.onnx`
  - Hoặc `.\models\export_and_validate.ps1`

### 3.6. `deploy/docker/`

- **docker-compose.yml:** Stack mặc định (detector CPU).
- **docker-compose.prod-gpu.yml:** Stack production dùng GPU (NVIDIA).
- **Build order khuyến nghị:** Postgres → detector → web → API (API cần Postgres cho bước build).
- **Script:** `build-and-run.ps1` (Windows): kiểm tra best.onnx, .env, start postgres, build từng image, up.

### 3.7. `docs/`

- **PROJECT-STRUCTURE.md:** Cấu trúc repo (file này).
- **architecture.md:** Kiến trúc hệ thống.
- **api-spec.md:** Mô tả API.
- **runbook.md:** Vận hành, sự cố.

---

## 4. Chuẩn hóa và lưu ý

- **Không commit:** `models/*.onnx`, `models/*.pt`, `.env`, `target/`, `node_modules/`, log/build tạm (đã thêm vào `.gitignore`).
- **Cấu hình bắt buộc khi chạy:** Có `configs/settings.yaml`, `configs/cameras.yaml`; trong Docker có volume mount `configs/` và `models/` (trong `models/` phải có `best.onnx`).
- **Docker context:** Build từ repo root, dockerfile nằm trong `deploy/docker/`; `.dockerignore` giảm context (bỏ node_modules, target, …).

---

## 5. Gửi cho đồng nghiệp

1. Clone repo.
2. Copy `deploy/docker/.env.example` → `deploy/docker/.env`, sửa DB, Telegram (nếu dùng).
3. Có file `models/best.onnx` (tự export từ `models/best.pt` hoặc nhận từ team).
4. Chạy từ `deploy/docker`:
   - CPU: `docker-compose up -d`
   - GPU: `docker-compose -f docker-compose.prod-gpu.yml up -d`
   - Hoặc dùng script: `.\build-and-run.ps1` (CPU), `.\build-and-run.ps1 -Gpu` (GPU).
5. Truy cập: Web http://localhost, API http://localhost:8080, đăng nhập mặc định `admin@example.com` / `admin123`.

Chi tiết API, kiến trúc và runbook: xem thêm trong `docs/`.
