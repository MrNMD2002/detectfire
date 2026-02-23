# Fire & Smoke Detection - API Build Commands
# Copy và chạy từng lệnh một, hoặc chạy cả file: .\build-commands.ps1

# Bước 1: Di chuyển vào thư mục docker
cd d:\MobifoneSolutions\Production\Detect_Fire_and_smoke\deploy\docker

# Bước 2: Xóa container cũ nếu có
docker rm -f fire-detect-db

# Bước 3: Start PostgreSQL
docker-compose -f docker-compose.yml up -d postgres

# Bước 4: Đợi PostgreSQL sẵn sàng (15 giây)
Write-Host "Waiting for PostgreSQL..." -ForegroundColor Yellow
Start-Sleep -Seconds 15

# Bước 5: Chạy migrations (Cách 1: Dùng Docker - Không cần cài đặt sqlx-cli)
Write-Host "Running migrations using Docker..." -ForegroundColor Yellow
docker run --rm -v "${PWD}\..\..\apps\api\migrations:/migrations" -e DATABASE_URL="postgres://fire_detect:devpassword@host.docker.internal:5432/fire_detect" rust:1.88-bookworm bash -c "cargo install sqlx-cli --no-default-features --features postgres && sqlx migrate run"

# Hoặc Cách 2: Cài đặt sqlx-cli và chạy từ host (nếu muốn)
# Write-Host "Installing sqlx-cli..." -ForegroundColor Yellow
# cargo install sqlx-cli --no-default-features --features postgres
# Write-Host "Running migrations..." -ForegroundColor Yellow
# $env:DATABASE_URL = "postgres://fire_detect:devpassword@localhost:5432/fire_detect"
# cd ..\..\apps\api
# sqlx migrate run
# cd ..\..\deploy\docker

# Bước 6: Build API với no-cache
Write-Host "Building API..." -ForegroundColor Yellow
docker-compose -f docker-compose.cpu.yml build --no-cache --build-arg "DATABASE_URL=postgres://fire_detect:devpassword@host.docker.internal:5432/fire_detect" api

# Bước 7: Kiểm tra kết quả
if ($LASTEXITCODE -eq 0) {
    Write-Host "Build completed successfully!" -ForegroundColor Green
} else {
    Write-Host "Build failed!" -ForegroundColor Red
}
