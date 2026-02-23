# Checklist: Stream camera hoạt động

Khi gặp lỗi **"Không kết nối được stream"**, kiểm tra lần lượt:

---

## 1. Detector service đang chạy

- Trong Docker: container **fire-detect-detector** phải **Running** (xanh), không Restarting.
- Kiểm tra: `docker ps` hoặc Docker Desktop.
- Nếu detector restart loop: xem `deploy/docker/DEBUG-DETECTOR.md` và đảm bảo có `models/best.onnx`.

---

## 2. `configs/cameras.yaml` có camera trùng với Web

- Trong **configs/cameras.yaml** mỗi camera có `camera_id` (vd: `cam-01`).
- **RTSP URL** trong file này là nguồn thật detector dùng để kéo stream; phải đúng và cùng mạng với máy chạy Docker.
- Ví dụ:

```yaml
cameras:
  - camera_id: "cam-01"
    site_id: "site-main"
    name: "Camera chính"
    rtsp_url: "rtsp://user:pass@10.1.1.174:554/cam/realmonitor?channel=1&subtype=0"
    enabled: true
```

---

## 3. Camera trong Web (API) có **Detector Camera ID** khớp

- Trong Web UI, mỗi camera (vd: "camera công ty") phải có **Detector Camera ID** = đúng với `camera_id` trong **configs/cameras.yaml**.
- Ví dụ: nếu trong `cameras.yaml` là `camera_id: "cam-01"` thì trong Web camera đó phải có **Detector Camera ID** = `cam-01`.
- Cách sửa:
  - Vào **Quản lý Camera** → bấm **"Sửa ID"** trên camera đó → nhập `cam-01` (hoặc đúng `camera_id` trong yaml) → **Lưu**.
- Khi **Thêm Camera** mới, nhập luôn **Detector Camera ID** (mặc định `cam-01` nếu chỉ có một camera trong yaml).

---

## 4. Luồng kết nối

```
Trình duyệt → Web → API (/api/cameras/{uuid}/stream/playlist.m3u8)
                         → API lấy detector_camera_id của camera (vd: cam-01)
                         → Gọi Detector: http://detector:51051/stream/cam-01/playlist.m3u8
                                    → Detector đọc cameras.yaml, tìm camera_id = "cam-01"
                                    → Khởi tạo pipeline RTSP từ rtsp_url của cam-01
                                    → Trả HLS về API → Web phát
```

Nếu **detector_camera_id** trong DB khác với **camera_id** trong yaml, detector không tìm thấy camera → lỗi stream.

---

## 5. Cùng mạng Docker (API phải resolve được hostname `detector`)

- API gọi stream qua `http://detector:51051/...`. Hostname **detector** chỉ resolve được khi API và Detector chạy trong **cùng một Docker Compose** (cùng network).
- **Cách đúng:** Chạy toàn bộ stack bằng một lệnh, ví dụ:
  - `docker-compose -f deploy/docker/docker-compose.prod-gpu.yml up -d`
  - Hoặc `.\deploy\docker\build-and-run.ps1` (nếu dùng script có sẵn).
- **Không** chạy detector bằng `docker run` riêng lẻ trừ khi bạn tự tạo network và alias `detector` cho container đó.
- Trong compose, service detector có `container_name: fire-detect-detector`; các service khác kết nối qua tên service **detector** (hostname).

---

## 6. Kiểm tra nhanh

- **Detector có đọc đúng config không:** xem log detector, có dòng load cameras và không báo lỗi RTSP.
- **API có gọi được detector không:** từ trong container API chạy `curl -s -o /dev/null -w "%{http_code}" http://detector:51051/stream/cam-01/playlist.m3u8` (thay cam-01 nếu cần). 200 = OK; 000 hoặc lỗi kết nối = sai mạng hoặc detector chưa listen.
- **Playlist chậm:** Detector có retry đọc playlist khi GStreamer vừa khởi tạo; lần đầu có thể mất vài giây.
- **RTSP cùng mạng:** máy chạy Docker (host) và camera cùng subnet / VLAN; nếu Docker dùng bridge network, host có thể ping được IP camera.

Sau khi sửa **Detector Camera ID** cho camera "camera công ty" thành `cam-01` (và detector đang chạy, cameras.yaml có `cam-01` với rtsp_url đúng), mở lại stream để thử.
