# Fire & Smoke Detection - Docker

## Chuẩn bị (lần đầu)

**Quan trọng:** API build cần PostgreSQL đang chạy và migrations đã được apply để sqlx compile-time check.

### Cách 1: Dùng script tự động (KHUYẾN NGHỊ)

```powershell
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker

# Script sẽ tự động:
# 1. Start PostgreSQL
# 2. Đợi database sẵn sàng
# 3. Chạy migrations
# 4. Verify schema
# 5. Build API với no-cache
.\build-api.ps1
```

### Cách 2: Build thủ công

```powershell
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker

# 1. Start postgres
docker-compose -f docker-compose.prod-cpu.yml up -d postgres

# 2. Đợi postgres sẵn sàng (khoảng 15 giây)
Start-Sleep -Seconds 15

# 3. Chạy migrations từ host (quan trọng!)
$env:DATABASE_URL = "postgres://fire_detect:devpassword@localhost:5432/fire_detect"
cd ..\..\apps\api
sqlx migrate run
cd ..\..\deploy\docker

# 4. Build API với DATABASE_URL
docker-compose -f docker-compose.prod-cpu.yml build --no-cache --build-arg DATABASE_URL="postgres://fire_detect:devpassword@host.docker.internal:5432/fire_detect" api

# 5. Start tất cả services
docker-compose -f docker-compose.prod-cpu.yml up -d
```

**Lưu ý:**

- **QUAN TRỌNG**: Migrations phải được chạy TRƯỚC khi build API
- Dockerfile sẽ tự động verify schema, nhưng tốt nhất là chạy migrations từ host trước
- Port 5432 phải được expose ra host để Docker build có thể connect qua `host.docker.internal`

### Cách 3: Dùng SQLX_OFFLINE (nếu đã có .sqlx cache)

```powershell
# Nếu đã chạy sqlx prepare và có .sqlx folder trong apps/api
cd apps\api
sqlx prepare --database-url "postgres://fire_detect:devpassword@localhost:5432/fire_detect"

# Sau đó build với SQLX_OFFLINE=true
cd ..\..\deploy\docker
$env:SQLX_OFFLINE="true"
docker-compose -f docker-compose.prod-cpu.yml build api
```

## Chạy nhanh (CPU - không cần GPU)

```powershell
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker

# Đảm bảo có file .env (đã tạo sẵn)
# Chỉnh DB_PASSWORD nếu cần

docker-compose -f docker-compose.prod-cpu.yml up -d --build
```

Sau khi chạy:

- **Web UI**: http://localhost (port 80)
- **API**: http://localhost:8080
- **PostgreSQL**: localhost:5432

Đăng nhập mặc định: `admin@example.com` / `admin123`

---

## Chạy với GPU (NVIDIA)

```powershell
docker-compose -f docker-compose.prod-gpu.yml up -d --build
```

**Yêu cầu:**
- Docker đã cài [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html)
- Driver NVIDIA trên host
- File `models/best.onnx` đã có (export từ `best.pt` bằng `models/export_onnx.py`)

**Dockerfile.detector (GPU):** Build dùng ONNX Runtime GPU cài sẵn (`ORT_STRATEGY=system`), cùng base image Ubuntu 22.04 cho build và runtime để tránh lỗi linker/GLIBC. Thư mục `/opt/onnxruntime` được copy sang runtime để container tìm thấy `libonnxruntime.so`.

---

## Chỉ chạy Database + API + Web (không detector)

Sửa `docker-compose.cpu.yml`: comment hoặc xóa service `detector`, và bỏ `depends_on: detector` của service `api`.

---

## Cấu hình camera

1. Đăng nhập Web → Cameras
2. Thêm camera với RTSP URL
3. **Quan trọng**: Gán `detector_camera_id: "cam-01"` (khớp với `configs/cameras.yaml`) để stream và event hoạt động
