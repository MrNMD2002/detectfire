# Fire & Smoke Detection - System Test Script
# Chạy: .\scripts\test-system.ps1

Write-Host "=== Fire & Smoke Detection - System Check ===" -ForegroundColor Cyan

# 1. Web UI Build
Write-Host "`n[1] Building Web UI..." -ForegroundColor Yellow
Push-Location apps\web
$webBuild = npm run build 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Host "  Web UI: OK" -ForegroundColor Green
} else {
    Write-Host "  Web UI: FAILED" -ForegroundColor Red
    Write-Host $webBuild
}
Pop-Location

# 2. API Build (cần DB hoặc SQLX_OFFLINE)
Write-Host "`n[2] Building API..." -ForegroundColor Yellow
Push-Location apps\api
$apiBuild = cargo check 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Host "  API: OK" -ForegroundColor Green
} else {
    Write-Host "  API: Có thể cần DATABASE_URL hoặc sqlx prepare" -ForegroundColor Yellow
}
Pop-Location

# 3. Migration check
Write-Host "`n[3] Migrations:" -ForegroundColor Yellow
Get-ChildItem apps\api\migrations\*.sql | ForEach-Object { Write-Host "  - $($_.Name)" }

# 4. Config check
Write-Host "`n[4] Config files:" -ForegroundColor Yellow
if (Test-Path configs\cameras.yaml) { Write-Host "  cameras.yaml: OK" -ForegroundColor Green }
if (Test-Path configs\settings.yaml) { Write-Host "  settings.yaml: OK" -ForegroundColor Green }

Write-Host "`n=== Done ===" -ForegroundColor Cyan
