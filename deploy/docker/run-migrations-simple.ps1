# Run database migrations - Simple version using psql directly
# Usage: .\run-migrations-simple.ps1

Write-Host "=== Running Database Migrations ===" -ForegroundColor Cyan

# Check if PostgreSQL container is running
$dbRunning = docker ps --filter "name=fire-detect-db" --format "{{.Names}}"
if (-not $dbRunning) {
    Write-Host "ERROR: PostgreSQL container 'fire-detect-db' is not running!" -ForegroundColor Red
    Write-Host "Please start it first: docker-compose -f docker-compose.cpu.yml up -d postgres" -ForegroundColor Yellow
    exit 1
}

Write-Host "PostgreSQL container is running: $dbRunning" -ForegroundColor Green

# Get migrations directory
$migrationsDir = Resolve-Path "..\..\apps\api\migrations"
Write-Host "Migrations directory: $migrationsDir" -ForegroundColor Gray

# Run migration 1
Write-Host ""
Write-Host "[1/2] Running migration 0001_initial.sql..." -ForegroundColor Yellow
$migration1 = Join-Path $migrationsDir "0001_initial.sql"
Get-Content $migration1 | docker exec -i fire-detect-db psql -U fire_detect -d fire_detect

if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Migration 1 failed!" -ForegroundColor Red
    exit 1
}
Write-Host "Migration 1 completed successfully!" -ForegroundColor Green

# Run migration 2
Write-Host ""
Write-Host "[2/2] Running migration 0002_detector_camera_id.sql..." -ForegroundColor Yellow
$migration2 = Join-Path $migrationsDir "0002_detector_camera_id.sql"
Get-Content $migration2 | docker exec -i fire-detect-db psql -U fire_detect -d fire_detect

if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Migration 2 failed!" -ForegroundColor Red
    exit 1
}
Write-Host "Migration 2 completed successfully!" -ForegroundColor Green

# Verify tables exist
Write-Host ""
Write-Host "Verifying schema..." -ForegroundColor Yellow
$tables = docker exec fire-detect-db psql -U fire_detect -d fire_detect -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('users', 'cameras', 'events');"

if ($tables -match "3") {
    Write-Host "Schema verified: all tables exist!" -ForegroundColor Green
} else {
    Write-Host "Warning: Expected 3 tables, found: $tables" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Migrations Completed ===" -ForegroundColor Cyan
