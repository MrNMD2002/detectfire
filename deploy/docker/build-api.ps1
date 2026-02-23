# Build API Service with PostgreSQL setup
# Usage: .\build-api.ps1

$ErrorActionPreference = "Stop"

Write-Host "=== Fire & Smoke Detection - API Build Setup ===" -ForegroundColor Cyan

# Check if we're in the right directory
if (-not (Test-Path "docker-compose.cpu.yml")) {
    Write-Host "ERROR: docker-compose.cpu.yml not found. Please run this script from deploy/docker directory." -ForegroundColor Red
    exit 1
}

# Step 1: Start PostgreSQL (remove old container if exists)
Write-Host ""
Write-Host "[1/5] Starting PostgreSQL..." -ForegroundColor Yellow
docker rm -f fire-detect-db 2>$null
docker-compose -f docker-compose.cpu.yml up -d postgres

# Step 2: Wait for PostgreSQL to be ready
Write-Host ""
Write-Host "[2/5] Waiting for PostgreSQL to be ready..." -ForegroundColor Yellow
$maxAttempts = 30
$attempt = 0
$ready = $false

while ($attempt -lt $maxAttempts -and -not $ready) {
    $attempt++
    try {
        $null = docker exec fire-detect-db pg_isready -U fire_detect 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  PostgreSQL is ready!" -ForegroundColor Green
            $ready = $true
        } else {
            Write-Host "  Attempt $attempt of $maxAttempts: Waiting..." -ForegroundColor Gray
            Start-Sleep -Seconds 2
        }
    } catch {
        Write-Host "  Attempt $attempt of $maxAttempts: Waiting..." -ForegroundColor Gray
        Start-Sleep -Seconds 2
    }
}

if (-not $ready) {
    Write-Host "ERROR: PostgreSQL did not become ready in time" -ForegroundColor Red
    exit 1
}

# Step 3: Run migrations manually
Write-Host ""
Write-Host "[3/5] Running database migrations..." -ForegroundColor Yellow
$dbUrl = "postgres://fire_detect:devpassword@localhost:5432/fire_detect"

# Check if sqlx-cli is available
$sqlxAvailable = Get-Command sqlx -ErrorAction SilentlyContinue
if (-not $sqlxAvailable) {
    Write-Host "  Installing sqlx-cli..." -ForegroundColor Gray
    cargo install sqlx-cli --no-default-features --features postgres
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to install sqlx-cli" -ForegroundColor Red
        exit 1
    }
}

Write-Host "  Running migrations with DATABASE_URL=$dbUrl" -ForegroundColor Gray
$env:DATABASE_URL = $dbUrl
Push-Location ..\..\apps\api
sqlx migrate run
$migrationExitCode = $LASTEXITCODE
Pop-Location

if ($migrationExitCode -ne 0) {
    Write-Host "ERROR: Migration failed" -ForegroundColor Red
    exit 1
}

Write-Host "  Migrations completed successfully!" -ForegroundColor Green

# Step 4: Verify schema
Write-Host ""
Write-Host "[4/5] Verifying database schema..." -ForegroundColor Yellow
$env:DATABASE_URL = $dbUrl
$verifyQuery = "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('users', 'cameras', 'events');"
$result = docker exec fire-detect-db psql -U fire_detect -d fire_detect -t -c $verifyQuery
if ($result -match "3") {
    Write-Host "  Schema verified: all tables exist" -ForegroundColor Green
} else {
    Write-Host "  Warning: Schema verification failed. Tables may not exist." -ForegroundColor Yellow
}

# Step 5: Build API (no cache to ensure new code is built)
Write-Host ""
Write-Host "[5/5] Building API service (no cache)..." -ForegroundColor Yellow
Write-Host "  Using DATABASE_URL=$dbUrl for SQLx compile-time checks" -ForegroundColor Gray

docker-compose -f docker-compose.cpu.yml build --no-cache --build-arg "DATABASE_URL=$dbUrl" api

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "API build completed successfully!" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "API build failed" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "=== Done ===" -ForegroundColor Cyan
