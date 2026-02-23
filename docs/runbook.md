# Fire & Smoke Detection - Runbook

Hướng dẫn vận hành hệ thống phát hiện cháy/khói.

## 1. Deployment

### Prerequisites

- Docker & Docker Compose
- NVIDIA Container Toolkit (cho GPU)
- PostgreSQL client (psql)

### Quick Start

```bash
# Clone repository
git clone https://github.com/your-org/fire-detect.git
cd fire-detect

# Copy và cấu hình environment
cp deploy/docker/.env.example deploy/docker/.env
# Edit .env với settings của bạn

# Start services
cd deploy/docker
docker compose up -d

# Kiểm tra logs
docker compose logs -f
```

### Kiểm tra Health

```bash
# API health
curl http://localhost:8080/health

# Grafana: http://localhost:3000 (admin/admin)
```

---

## 2. Configuration

### Thêm Camera mới

**Option 1: Via Web UI**

1. Đăng nhập http://localhost
2. Vào Cameras > Add Camera
3. Điền thông tin RTSP

**Option 2: Via API**

```bash
curl -X POST http://localhost:8080/api/cameras \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "site_id": "site-a",
    "name": "Camera Nhà kho",
    "rtsp_url": "rtsp://user:pass@192.168.1.100:554/stream",
    "fps_sample": 3,
    "conf_fire": 0.5,
    "conf_smoke": 0.4
  }'
```

**Option 3: Via cameras.yaml**

```yaml
# configs/cameras.yaml
cameras:
  - camera_id: cam-01
    site_id: site-a
    name: "Camera Nhà kho"
    rtsp_url: "${RTSP_CAM_01}"
    enabled: true
```

### Cấu hình Telegram

1. Tạo bot với @BotFather
2. Lấy bot token
3. Thêm bot vào group và lấy chat_id
4. Cập nhật .env:

```
TELEGRAM_BOT_TOKEN=123456:ABC-DEF...
TELEGRAM_CHAT_ID=-1001234567890
```

### Tune Detection Parameters

| Parameter      | Mô tả                | Khuyến nghị           |
| -------------- | -------------------- | --------------------- |
| `fps_sample`   | FPS lấy mẫu          | 2-5 (cao = nhiều GPU) |
| `conf_fire`    | Ngưỡng fire          | 0.4-0.6               |
| `conf_smoke`   | Ngưỡng smoke         | 0.3-0.5               |
| `window_size`  | Cửa sổ voting        | 5-15 frames           |
| `fire_hits`    | Min fire detections  | 2-4                   |
| `cooldown_sec` | Cooldown giữa alerts | 30-120s               |

---

## 3. Monitoring

### Grafana Dashboard

1. Truy cập http://localhost:3000
2. Login: admin / admin
3. Vào Fire & Smoke Detection dashboard

### Key Metrics

- **Fire/Smoke Events (1h)**: Số sự kiện trong 1 giờ
- **Stream Events**: Camera up/down events
- **Errors**: Error logs

### LogQL Queries

```logql
# Tất cả fire events
{project="fire-detect", event_type="fire"}

# Errors từ detector
{service="detector", level="error"}

# Events từ camera cụ thể
{camera_id="cam-01"}

# Count events per camera
sum by (camera_id) (count_over_time({event_type=~"fire|smoke"} [5m]))
```

---

## 4. Troubleshooting

### Camera không stream

**Triệu chứng:** Camera status = FAILED

**Kiểm tra:**

```bash
# Test RTSP connection
ffprobe -rtsp_transport tcp rtsp://user:pass@192.168.1.100:554/stream

# Check detector logs
docker logs fire-detect-detector 2>&1 | grep "cam-01"
```

**Giải pháp:**

1. Kiểm tra network connectivity
2. Verify RTSP credentials
3. Restart detector: `docker restart fire-detect-detector`

### False Positives quá nhiều

**Giải pháp:**

1. Tăng `conf_fire` / `conf_smoke` (0.6-0.7)
2. Tăng `window_size` (15-20)
3. Tăng `fire_hits` / `smoke_hits` (4-5)

### Missed Detections (False Negatives)

**Giải pháp:**

1. Giảm `conf_fire` / `conf_smoke` (0.3-0.4)
2. Giảm `window_size` (5-8)
3. Tăng `fps_sample` (4-5 nhưng cẩn thận GPU)

### GPU OOM

**Triệu chứng:** "CUDA out of memory"

**Giải pháp:**

1. Giảm số cameras active
2. Giảm `imgsz` (640 → 480)
3. Giảm `fps_sample`
4. Upgrade GPU

### Database Issues

```bash
# Check PostgreSQL logs
docker logs fire-detect-db

# Connect to database
docker exec -it fire-detect-db psql -U fire_detect -d fire_detect

# Check connections
SELECT count(*) FROM pg_stat_activity WHERE datname = 'fire_detect';
```

---

## 5. Maintenance

### Backup Database

```bash
# Backup
docker exec fire-detect-db pg_dump -U fire_detect fire_detect > backup.sql

# Restore
docker exec -i fire-detect-db psql -U fire_detect fire_detect < backup.sql
```

### Cleanup Old Snapshots

```bash
# Delete snapshots older than 30 days
find /path/to/snapshots -type f -mtime +30 -delete
```

### Cleanup Old Events

```sql
-- Delete events older than 90 days
DELETE FROM events WHERE created_at < NOW() - INTERVAL '90 days';
```

### Update System

```bash
cd deploy/docker
docker compose pull
docker compose up -d --force-recreate
```

---

## 6. Emergency Procedures

### Disable All Alerts

```bash
# Via API
curl -X PUT http://localhost:8080/api/settings/telegram \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"enabled": false}'
```

### Stop All Detection

```bash
docker stop fire-detect-detector
```

### Full System Restart

```bash
cd deploy/docker
docker compose down
docker compose up -d
```

---

## 7. Contact

- **On-call**: +84-xxx-xxx-xxx
- **Email**: fire-detect@example.com
- **Slack**: #fire-detect-ops
