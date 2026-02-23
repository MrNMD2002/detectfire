# Quick Start - Build API Service

## Bước 1: Start PostgreSQL

```powershell
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker
docker-compose -f docker-compose.prod-cpu.yml up -d postgres
```

Đợi 15 giây để PostgreSQL sẵn sàng.

## Bước 2: Chạy Migrations (Chọn 1 trong 3 cách)

### Cách A: Dùng psql trực tiếp (NHANH NHẤT - Khuyến nghị)

```powershell
.\run-migrations-simple.ps1
```

### Cách B: Dùng Docker với sqlx-cli

```powershell
.\run-migrations.ps1
```

**Lưu ý:** Cách này sẽ mất vài phút để cài đặt `cargo` và `sqlx-cli` lần đầu tiên.

### Cách C: Chạy SQL trực tiếp

```powershell
# Migration 1
Get-Content ..\..\apps\api\migrations\0001_initial.sql | docker exec -i fire-detect-db psql -U fire_detect -d fire_detect

# Migration 2
Get-Content ..\..\apps\api\migrations\0002_detector_camera_id.sql | docker exec -i fire-detect-db psql -U fire_detect -d fire_detect
```

## Bước 3: Build API

```powershell
docker-compose -f docker-compose.prod-cpu.yml build --no-cache --build-arg "DATABASE_URL=postgres://fire_detect:devpassword@host.docker.internal:5432/fire_detect" api
```

## Bước 4: Start tất cả services

```powershell
docker-compose -f docker-compose.prod-cpu.yml up -d
```

## Kiểm tra

```powershell
# Xem logs
docker logs fire-detect-api

# Kiểm tra health
curl http://localhost:8080/health
```
