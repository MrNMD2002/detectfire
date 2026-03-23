# =============================================================================
# setup_runner.ps1 — Tự động cài GitHub Actions Self-hosted Runner
# Repo: https://github.com/MrNMD2002/detectfire
#
# Cách dùng:
#   PowerShell (Admin) > .\scripts\setup_runner.ps1 -Token <TOKEN_TU_GITHUB>
#
# Lấy token tại:
#   https://github.com/MrNMD2002/detectfire/settings/actions/runners/new?runnerOs=windows
# =============================================================================

param(
    [Parameter(Mandatory=$true)]
    [string]$Token
)

$ErrorActionPreference = "Stop"

$REPO_URL   = "https://github.com/MrNMD2002/detectfire"
$RUNNER_DIR = "C:\actions-runner"
$RUNNER_NAME = "fire-detection-runner"

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  GitHub Actions Self-hosted Runner"
Write-Host "  Repo: MrNMD2002/detectfire"
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# ── Bước 1: Lấy phiên bản runner mới nhất ────────────────────────────────────
Write-Host "[1/5] Kiểm tra phiên bản runner mới nhất..." -ForegroundColor Yellow
$latestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/actions/runner/releases/latest"
$version = $latestRelease.tag_name.TrimStart("v")
$downloadUrl = "https://github.com/actions/runner/releases/download/v${version}/actions-runner-win-x64-${version}.zip"
Write-Host "      Phiên bản: v$version" -ForegroundColor Green

# ── Bước 2: Tạo thư mục và download ──────────────────────────────────────────
Write-Host "[2/5] Tạo thư mục $RUNNER_DIR và download runner..." -ForegroundColor Yellow

if (Test-Path $RUNNER_DIR) {
    Write-Host "      Thư mục đã tồn tại — xoá để cài lại..." -ForegroundColor DarkYellow
    Remove-Item -Recurse -Force $RUNNER_DIR
}
New-Item -ItemType Directory -Path $RUNNER_DIR | Out-Null

$zipFile = "$RUNNER_DIR\runner.zip"
Write-Host "      Downloading từ: $downloadUrl"
Invoke-WebRequest -Uri $downloadUrl -OutFile $zipFile -UseBasicParsing

Write-Host "      Giải nén..." -ForegroundColor Green
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($zipFile, $RUNNER_DIR)
Remove-Item $zipFile

# ── Bước 3: Configure runner ──────────────────────────────────────────────────
Write-Host "[3/5] Cấu hình runner..." -ForegroundColor Yellow
Set-Location $RUNNER_DIR

& "$RUNNER_DIR\config.cmd" `
    --url $REPO_URL `
    --token $Token `
    --name $RUNNER_NAME `
    --work "_work" `
    --labels "self-hosted,Windows,x64,gpu" `
    --unattended `
    --replace

Write-Host "      Runner '$RUNNER_NAME' đã cấu hình xong." -ForegroundColor Green

# ── Bước 4: Cài như Windows Service ──────────────────────────────────────────
Write-Host "[4/5] Cài đặt runner như Windows Service (tự khởi động)..." -ForegroundColor Yellow

& "$RUNNER_DIR\svc.sh" install 2>$null
if ($LASTEXITCODE -ne 0) {
    # Fallback: dùng PowerShell service install
    & "$RUNNER_DIR\config.cmd" --runasservice 2>$null
}

# Khởi động service
$svcName = "actions.runner.MrNMD2002-detectfire.$RUNNER_NAME"
$svc = Get-Service -Name "*actions.runner*" -ErrorAction SilentlyContinue
if ($svc) {
    Start-Service $svc.Name
    Write-Host "      Service '$($svc.Name)' đã khởi động." -ForegroundColor Green
} else {
    Write-Host "      Không tìm thấy service — chạy runner trực tiếp..." -ForegroundColor DarkYellow
    Start-Process -FilePath "$RUNNER_DIR\run.cmd" -WindowStyle Minimized
    Write-Host "      Runner đang chạy ở background." -ForegroundColor Green
}

# ── Bước 5: Kiểm tra ─────────────────────────────────────────────────────────
Write-Host "[5/5] Kiểm tra kết quả..." -ForegroundColor Yellow
Start-Sleep -Seconds 3

$svc = Get-Service -Name "*actions.runner*" -ErrorAction SilentlyContinue
if ($svc) {
    Write-Host "      Service status: $($svc.Status)" -ForegroundColor Green
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  DONE! Runner đã sẵn sàng." -ForegroundColor Green
Write-Host ""
Write-Host "  Kiểm tra tại:" -ForegroundColor White
Write-Host "  https://github.com/MrNMD2002/detectfire/settings/actions/runners" -ForegroundColor White
Write-Host ""
Write-Host "  Label: self-hosted, Windows, x64, gpu" -ForegroundColor White
Write-Host "========================================" -ForegroundColor Cyan
