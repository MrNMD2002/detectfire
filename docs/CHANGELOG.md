# Changelog — Fire & Smoke Detection System

> Tài liệu tổng kết các thay đổi, lý do thay đổi và so sánh với phiên bản trước.
> Cập nhật lần cuối: 2026-03-02

---

## 1. Kiến trúc Stream (Live View Camera)

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **Giao thức stream** | HLS (playlist .m3u8) | MJPEG over HTTP | HLS có buffer 6-10s → độ trễ cao, không phù hợp giám sát real-time |
| **Mô hình dữ liệu** | Pull: worker liên tục `try_pull_sample()` trong vòng lặp | Push: GStreamer callback → `broadcast::Sender<Arc<Frame>>` | Pull tốn CPU khi idle, dễ miss frame; Push chỉ xử lý khi có frame thật |
| **Latency** | ~6-10 giây | ~150-200ms | Giảm ~30-50x — quan sát đám cháy gần real-time |
| **Client-side** | `<video>` tag + HLS.js | `fetch()` + JS parse JPEG markers (SOI/EOI) → blob URL → `<img>` | Không cần thư viện HLS, parse MJPEG boundary thủ công để hiển thị từng frame |

### Files thay đổi
- `apps/detector/src/camera/pipeline.rs` — thêm `set_callbacks` appsink, push `Arc<Frame>` vào broadcast channel
- `apps/detector/src/camera/worker.rs` — subscribe `broadcast::Receiver`, bỏ pull loop
- `apps/detector/src/stream/server.rs` — MJPEG endpoint dùng `BroadcastStream` + JPEG encode
- `apps/api/src/routes/stream.rs` — proxy MJPEG từ detector, auth bằng header hoặc `?token=`
- `apps/web/src/components/CameraStreamModal.tsx` — JS JPEG parser thay thế HLS player

---

## 2. Inference Engine (AI Detection)

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **Execution Provider** | CUDA EP (FP32) | TensorRT EP (FP16) + CUDA EP fallback | TRT compile model → optimized engine dùng Tensor Cores, FP16 giảm memory bandwidth |
| **Tốc độ inference** | ~100-150ms/frame (ước tính CUDA FP32) | ~47ms/frame | Nhanh hơn ~3x vs CUDA; ~20x vs CPU |
| **Throughput** | ~7-10 FPS processing | ~21 FPS processing | Sample rate 3 FPS → GPU rảnh 85%, đủ scale thêm 5-6 camera |
| **Engine cache** | Không có | Volume `trt_engines` persist qua restart | Lần đầu compile ~2-3 phút, các lần sau load cache < 5 giây |
| **FP16 ONNX thủ công** | FP32 | Vẫn FP32 (TRT tự convert nội bộ) | Thử convert ONNX → FP16 bằng `onnxconverter-common` thất bại: Cast node trong YOLO NMS không tương thích type. TRT xử lý FP16 internally qua `ORT_TENSORRT_FP16_ENABLE=1` |

### Benchmark thực đo

| Mode | Inference/frame | So sánh |
|---|---|---|
| CPU (baseline) | ~974ms | 1x |
| TRT FP16 (hiện tại) | ~47ms avg (min 45ms, max 76ms) | **~20x nhanh hơn CPU** |

### Files thay đổi
- `apps/detector/Cargo.toml` — thêm `ort/tensorrt` vào feature `gpu`
- `apps/detector/src/inference/engine.rs` — đăng ký `ep::TensorRT::default()` ưu tiên trước `ep::CUDA`
- `deploy/docker/docker-compose.yml` — thêm env vars TRT + volume `trt_engines`
- `deploy/docker/Dockerfile.detector` — thêm `libnvinfer10`, `libnvinfer-plugin10`, `libnvonnxparsers10`

### Env vars TRT (docker-compose.yml)
```yaml
ORT_TENSORRT_FP16_ENABLE: "1"
ORT_TENSORRT_ENGINE_CACHE_ENABLE: "1"
ORT_TENSORRT_CACHE_PATH: /app/trt_engines/
```

---

## 3. Bounding Boxes trên Snapshot / Telegram

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **Hình ảnh gửi Telegram** | Ảnh trống, không có box | Ảnh có bounding box màu đỏ (fire) / cam (smoke) + label confidence | Bug: coord model output là pixel [0,640], code clamp về [0,1] → box collapse |
| **Root cause** | `det[0].clamp(0.0, 1.0)` với giá trị 100.0 → 1.0 → bbox tại góc ảnh, ngoài vùng nhìn thấy | `(det[0] / 640.0).clamp(0.0, 1.0)` — normalize đúng trước khi clamp | End-to-end NMS model của YOLO output pixel coords, không phải normalized |
| **Font render** | Không có | DejaVuSans-Bold.ttf qua thư viện `ab_glyph` | `imageproc 0.24` dùng `ab_glyph` thay `rusttype` (API cũ đã deprecated) |
| **Detection logic** | Đúng (event vẫn fire, Telegram vẫn nhận ảnh) | Đúng + có visual | Bug chỉ ảnh hưởng phần vẽ box, không ảnh hưởng accuracy |

### Files thay đổi
- `apps/detector/src/inference/postprocess.rs` — fix `parse_detection()`: chia 640 trước khi clamp
- `apps/detector/src/event/publisher.rs` — thêm `draw_bounding_boxes()` dùng `imageproc` + `ab_glyph`
- `deploy/docker/Dockerfile.detector` — thêm `fonts-dejavu-core` vào runtime image

---

## 4. Hot Reload Camera Config

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **Thêm/sửa camera** | Phải restart container thủ công, gây downtime | Thay đổi qua Web UI → API → gRPC → Detector tự reload | Giảm downtime trên production khi thêm/sửa camera |
| **Flow** | Static config đọc một lần khi khởi động | CRUD camera trên web → API ghi `cameras.yaml` → gRPC `reload_config` → `camera_manager.reload_cameras()` | Camera đang chạy không bị ngắt khi reload |

### Files thay đổi
- `apps/api/src/camera_sync.rs` *(mới)* — ghi `cameras.yaml` từ DB (RTSP URL được decrypt)
- `apps/detector/src/camera/manager.rs` — thêm `reload_cameras()`: start/stop worker theo config mới
- `apps/detector/src/grpc/detector_grpc.rs` — implement `reload_config()` gRPC handler

### Bug compile đã fix
`parking_lot::RwLockWriteGuard` (non-Send) giữ qua `.await` → compile error.
```rust
// Sai: guard sống trong if block, vượt qua await
if let Some(mut w) = self.workers.write().remove(id) { w.stop().await; }

// Đúng: drop guard trước, rồi mới await
let w = self.workers.write().remove(id);
if let Some(mut w) = w { w.stop().await; }
```

---

## 5. API & Events

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **GET /api/events** | Trả `Event[]` | Trả `{ data: Event[], total: number }` | Hỗ trợ pagination — biết tổng số bản ghi không cần fetch hết |
| **Snapshot** | Không lưu | Lưu `/app/snapshots/{cam_id}/{ts}.jpg`, serve qua `GET /api/snapshots/{cam}/{file}` | Xem lại bằng chứng sự cố sau khi xảy ra |
| **GET /api/events/count** | Không có | Có — đếm theo bộ lọc (camera, thời gian) | Dashboard stats hiển thị số sự kiện hôm nay |
| **Settings Telegram** | Hardcode trong `.env` | `GET /api/settings/telegram` (masked) + `POST /api/settings/telegram/test` | Thay đổi Telegram config không cần restart, test ngay trên UI |

### Breaking change
Frontend phải dùng `response.data` thay vì trực tiếp array:
```typescript
// Trước
const events: Event[] = await getEvents();

// Sau
const result = await getEvents(); // { data: Event[], total: number }
const events = result.data;
```

---

## 6. Build & Dockerfile

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **Cargo build pipe** | `cargo build 2>&1 \| tee /tmp/build.log && ...` | `bash -c 'set -euo pipefail; cargo build \| tee /tmp/build.log'` | `tee` nuốt exit code của `cargo` → build lỗi vẫn tạo ra binary cũ, không báo lỗi |
| **SQLx offline** | Cần kết nối DB lúc build | `SQLX_OFFLINE=true` + copy `.sqlx/` vào image | Build trong Docker CI không cần DB live; query metadata commit vào repo |
| **TRT runtime libs** | Không có | `libnvinfer10`, `libnvinfer-plugin10`, `libnvonnxparsers10` | ORT TensorRT EP cần TensorRT runtime (~300MB) để khởi động |
| **ORT dynamic libs** | Không rõ ràng | Download explicit `onnxruntime-linux-x64-gpu-1.23.2.tgz` trong runtime stage | `load-dynamic` = không link static → phải có `.so` riêng trong image |
| **Base image runtime** | Ubuntu 22.04 (glibc 2.35) | Ubuntu 24.04 (glibc 2.39) | ORT 1.20+ prebuilt lib yêu cầu glibc ≥ 2.39 |

---

## 7. Camera H.265 Support

| Hạng mục | Trước | Sau | Lý do thay đổi |
|---|---|---|---|
| **GStreamer decoder** | Hardcode H.264: `rtph264depay ! h264parse ! avdec_h264` | Tự chọn theo trường `codec`: `avdec_h265` nếu `codec: h265` | Camera thực tế (`cam-01`) stream HEVC/H.265 → decoder H.264 báo lỗi không decode được |
| **Config per-camera** | Không có trường `codec` | `codec: h265` trong `cameras.yaml` | Mỗi camera có thể dùng codec khác nhau, cần cấu hình riêng |

### Files thay đổi
- `apps/detector/src/config/models.rs` — thêm field `codec: Option<String>`
- `apps/detector/src/camera/pipeline.rs` — switch decoder dựa theo `codec`
- `configs/cameras.yaml` — thêm `codec: h265` cho `cam-01`

---

## Tổng quan tác động

| Chỉ số | Trước | Sau | Cải thiện |
|---|---|---|---|
| Latency live view | ~6-10 giây | ~200ms | **~30-50x** |
| Inference speed | ~100-150ms/frame | ~47ms/frame | **~3x** (vs CUDA), **~20x** (vs CPU) |
| Bounding box Telegram | Không có | Có (fire=đỏ, smoke=cam) | ✅ |
| Hot reload camera | Không có (restart thủ công) | Có (qua Web UI) | ✅ |
| Snapshot lưu trữ | Không có | Có | ✅ |
| Camera H.265/HEVC | Không chạy (crash) | Chạy ổn định | ✅ |
| GPU utilization | ~100% khi inference | ~15% (3 FPS sample) | Dư ~85% cho 5-6 camera thêm |
