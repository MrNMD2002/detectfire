# Run database migrations using Docker (không cần cài đặt sqlx-cli)
# Usage: .\run-migrations.ps1

Write-Host "Running database migrations using Docker..." -ForegroundColor Yellow

# Get absolute path to migrations directory
$migrationsPath = Resolve-Path "..\..\apps\api\migrations"

Write-Host "Migrations path: $migrationsPath" -ForegroundColor Gray

# Run migrations using Docker
docker run --rm `
    -v "${migrationsPath}:/migrations" `
    -e DATABASE_URL="postgres://fire_detect:devpassword@host.docker.internal:5432/fire_detect" `
    rust:1.88-bookworm `
    bash -c "cargo install sqlx-cli --no-default-features --features postgres --quiet && sqlx migrate run"

if ($LASTEXITCODE -eq 0) {
    Write-Host "Migrations completed successfully!" -ForegroundColor Green
} else {
    Write-Host "Migration failed!" -ForegroundColor Red
    exit 1
}
