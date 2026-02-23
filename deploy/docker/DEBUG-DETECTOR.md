# Debug Detector Restart Loop

## 1. Xem log container

```powershell
cd deploy\docker
docker logs fire-detect-detector 2>&1
```

Nếu thấy:
- **`[detector] Starting...`** rồi dừng → có thể lỗi khi load thư viện (thiếu .so) hoặc panic rất sớm.
- **`ERROR: Model file not found`** → volume `models` không mount đúng hoặc không có `best.onnx` trong `models/`.
- **`FATAL: No cameras configured`** → `configs/cameras.yaml` trống hoặc không load được.
- **`FATAL: ...`** khác → đọc message để biết lỗi (config, inference, v.v.).

## 2. Chạy detector bằng tay (để xem lỗi trực tiếp)

```powershell
cd deploy\docker
docker run --rm --gpus all `
  -v "${PWD}\..\..\configs:/app/configs:ro" `
  -v "${PWD}\..\..\models:/app/models:ro" `
  -e CONFIG_DIR=/app/configs `
  -e RUST_LOG=debug `
  docker-detector
```

Nếu thiếu thư viện, thường sẽ thấy dạng:  
`error while loading shared libraries: libonnxruntime.so.xxx: cannot open shared object file`

## 3. Kiểm tra volume models

Trên host phải có file:

```
Detect_Fire_and_smoke/models/best.onnx
```

Trong container đường dẫn là `/app/models/best.onnx`. Kiểm tra:

```powershell
docker run --rm -v "${PWD}\..\..\models:/app/models:ro" alpine ls -la /app/models/
```

Phải thấy `best.onnx` trong danh sách.

## 4. Rebuild detector sau khi sửa code/Dockerfile

```powershell
cd deploy\docker
docker-compose -f docker-compose.prod-gpu.yml build --no-cache detector
docker-compose -f docker-compose.prod-gpu.yml up -d detector
docker logs -f fire-detect-detector
```

## 5. Chạy không dùng GPU (thử CPU)

Nếu nghi ngờ lỗi do GPU/CUDA, chạy stack CPU:

```powershell
docker-compose -f docker-compose.yml up -d
docker logs -f fire-detect-detector
```
