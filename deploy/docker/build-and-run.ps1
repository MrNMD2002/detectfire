
# Fire & Smoke Detection - Build and Run Script
# Usage: .\build-and-run.ps1 [-Gpu] [-NoBuild] [-Clean]
# 
# Steps:
#   1. Start postgres, wait for health check
#   2. Build detector + web (no database needed)
#   3. Build API (needs postgres for sqlx compile-time checks)
#   4. Start all services

param(
    [switch]$Gpu,        # Use GPU Dockerfile for detector
    [switch]$NoBuild,    # Skip build, just start services
    [switch]$Clean       # Clean rebuild (--no-cache)
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$composeFile = if ($Gpu) { "docker-compose.prod-gpu.yml" } else { "docker-compose.yml" }
Write-Host "=== Fire & Smoke Detection - Docker Build ===" -ForegroundColor Cyan
Write-Host "Compose file: $composeFile" -ForegroundColor Yellow
Write-Host "GPU mode: $Gpu" -ForegroundColor Yellow

# Check prerequisites
Write-Host "`n[1/6] Checking prerequisites..." -ForegroundColor Green

# Check best.onnx exists
$modelPath = Join-Path $PSScriptRoot "..\..\models\best.onnx"
if (-not (Test-Path $modelPath)) {
    Write-Host "ERROR: models/best.onnx not found!" -ForegroundColor Red
    Write-Host "Export: python models/export_onnx.py --weights models/best.pt --output models/best.onnx" -ForegroundColor Yellow
    exit 1
}
Write-Host "  OK: models/best.onnx found ($(((Get-Item $modelPath).Length / 1MB).ToString('F1')) MB)" -ForegroundColor Gray

# Check .env
$envPath = Join-Path $PSScriptRoot ".env"
if (-not (Test-Path $envPath)) {
    Write-Host "  Creating .env from .env.example..." -ForegroundColor Yellow
    Copy-Item (Join-Path $PSScriptRoot ".env.example") $envPath
}
Write-Host "  OK: .env exists" -ForegroundColor Gray

# Check GPU if requested
if ($Gpu) {
    try {
        docker run --rm --gpus all nvidia/cuda:12.2.2-base-ubuntu22.04 nvidia-smi 2>&1 | Out-Null
        Write-Host "  OK: NVIDIA GPU available" -ForegroundColor Gray
    } catch {
        Write-Host "WARNING: GPU not available, falling back to CPU" -ForegroundColor Yellow
        $Gpu = $false
        $composeFile = "docker-compose.yml"
    }
}

if ($NoBuild) {
    Write-Host "`n[SKIP] Skipping build, starting services..." -ForegroundColor Yellow
    docker-compose -f $composeFile up -d
    Write-Host "`n=== Services started ===" -ForegroundColor Green
    docker-compose -f $composeFile ps
    exit 0
}

# Step 2: Start postgres first
Write-Host "`n[2/6] Starting PostgreSQL..." -ForegroundColor Green
docker-compose -f $composeFile up -d postgres

# Wait for postgres health check
Write-Host "  Waiting for PostgreSQL to be healthy..." -ForegroundColor Gray
$maxWait = 60
$waited = 0
while ($waited -lt $maxWait) {
    $health = docker inspect --format='{{.State.Health.Status}}' fire-detect-db 2>$null
    if ($health -eq "healthy") {
        Write-Host "  OK: PostgreSQL is healthy" -ForegroundColor Gray
        break
    }
    Start-Sleep -Seconds 2
    $waited += 2
    if ($waited % 10 -eq 0) {
        Write-Host "  Still waiting... ($waited/$maxWait sec)" -ForegroundColor Gray
    }
}
if ($waited -ge $maxWait) {
    Write-Host "ERROR: PostgreSQL health check timeout!" -ForegroundColor Red
    docker logs fire-detect-db --tail 20
    exit 1
}

# Step 3: Build detector (no database needed)
Write-Host "`n[3/6] Building detector..." -ForegroundColor Green
$buildArgs = @("-f", $composeFile, "build")
if ($Clean) { $buildArgs += "--no-cache" }
$buildArgs += "detector"
docker-compose @buildArgs
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Detector build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "  OK: Detector built" -ForegroundColor Gray

# Step 4: Build web (no database needed)
Write-Host "`n[4/6] Building web..." -ForegroundColor Green
$buildArgs = @("-f", $composeFile, "build")
if ($Clean) { $buildArgs += "--no-cache" }
$buildArgs += "web"
docker-compose @buildArgs
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Web build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "  OK: Web built" -ForegroundColor Gray

# Step 5: Build API (needs postgres running for sqlx compile-time checks)
Write-Host "`n[5/6] Building API (connects to postgres for schema validation)..." -ForegroundColor Green
$buildArgs = @("-f", $composeFile, "build")
if ($Clean) { $buildArgs += "--no-cache" }
$buildArgs += "api"
docker-compose @buildArgs
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: API build failed!" -ForegroundColor Red
    Write-Host "  Make sure PostgreSQL is accessible from Docker build context" -ForegroundColor Yellow
    Write-Host "  Check: docker logs fire-detect-db" -ForegroundColor Yellow
    exit 1
}
Write-Host "  OK: API built" -ForegroundColor Gray

# Step 6: Start all services
Write-Host "`n[6/6] Starting all services..." -ForegroundColor Green
docker-compose -f $composeFile up -d
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Failed to start services!" -ForegroundColor Red
    exit 1
}

# Show status
Start-Sleep -Seconds 5
Write-Host "`n=== Services Status ===" -ForegroundColor Cyan
docker-compose -f $composeFile ps

Write-Host "`n=== Access Points ===" -ForegroundColor Cyan
Write-Host "  Web UI:  http://localhost" -ForegroundColor White
Write-Host "  API:     http://localhost:8080" -ForegroundColor White
Write-Host "  Health:  http://localhost:8080/health" -ForegroundColor White
Write-Host "  DB:      localhost:5432" -ForegroundColor White

if ($Gpu) {
    Write-Host "`n=== GPU Info ===" -ForegroundColor Cyan
    Write-Host "  Detector is using NVIDIA GPU (CUDA)" -ForegroundColor White
}

Write-Host "`n=== Logs ===" -ForegroundColor Cyan
Write-Host "  All:      docker-compose -f $composeFile logs -f" -ForegroundColor Gray
Write-Host "  Detector: docker logs -f fire-detect-detector" -ForegroundColor Gray
Write-Host "  API:      docker logs -f fire-detect-api" -ForegroundColor Gray
Write-Host "  Web:      docker logs -f fire-detect-web" -ForegroundColor Gray

Write-Host "`nDone!" -ForegroundColor Green
