# Hướng dẫn Build API Service

## Cách 1: Chạy từng lệnh (Đơn giản nhất)

Mở PowerShell và chạy từng lệnh sau:

```powershell
# 1. Di chuyển vào thư mục docker
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker

# 2. Xóa container cũ nếu có
docker rm -f fire-detect-db

# 3. Start PostgreSQL
docker-compose -f docker-compose.cpu.yml up -d postgres

# 4. Đợi PostgreSQL sẵn sàng (15 giây)
Start-Sleep -Seconds 15

# 5. Chạy migrations (Có 2 cách)

# Cách 1: Dùng Docker (Không cần cài đặt sqlx-cli) - KHUYẾN NGHỊ
.\run-migrations.ps1

# Hoặc Cách 2: Cài đặt sqlx-cli và chạy từ host
# cargo install sqlx-cli --no-default-features --features postgres
# $env:DATABASE_URL = "postgres://fire_detect:devpassword@localhost:5432/fire_detect"
# cd ..\..\apps\api
# sqlx migrate run
# cd ..\..\deploy\docker

# 6. Build API với no-cache
docker-compose -f docker-compose.cpu.yml build --no-cache --build-arg "DATABASE_URL=postgres://fire_detect:devpassword@host.docker.internal:5432/fire_detect" api
```

## Cách 2: Chạy file script

```powershell
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker
.\build-commands.ps1
```

## Cách 3: Build tất cả services (sau khi đã build API)

```powershell
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker
docker-compose -f docker-compose.cpu.yml up -d --build
```

## Lưu ý quan trọng

1. **PostgreSQL phải chạy trước** khi build API
2. **Migrations phải được chạy từ host** trước khi build
3. **Port 5432 phải được expose** ra host
4. Dùng `--no-cache` để đảm bảo code mới được build

## Troubleshooting

### Lỗi: "relation cameras does not exist"
- Đảm bảo migrations đã chạy: `sqlx migrate run`
- Kiểm tra database: `docker exec fire-detect-db psql -U fire_detect -d fire_detect -c "\dt"`

### Lỗi: "Cannot connect to database"
- Kiểm tra PostgreSQL đang chạy: `docker ps | grep fire-detect-db`
- Kiểm tra port 5432: `netstat -an | findstr 5432`

### Lỗi: "sqlx: command not found"
- Cài đặt sqlx-cli: `cargo install sqlx-cli --no-default-features --features postgres`
