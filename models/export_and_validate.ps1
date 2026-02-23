# Export YOLOv10 to ONNX and validate
# Prerequisites: pip install -r requirements.txt
# Requires: best.pt in models/ folder (from YOLO training)

$ErrorActionPreference = "Stop"
$modelsDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "=== Fire & Smoke Model Export ===" -ForegroundColor Cyan
Write-Host ""

$ptPath = Join-Path $modelsDir "best.pt"
$onnxPath = Join-Path $modelsDir "best.onnx"

if (-not (Test-Path $ptPath)) {
    Write-Host "ERROR: best.pt not found at $ptPath" -ForegroundColor Red
    Write-Host "You need to train a YOLO model first, or copy best.pt to models/ folder." -ForegroundColor Yellow
    Write-Host ""
    Write-Host "If you have best.pt elsewhere, copy it:" -ForegroundColor Gray
    Write-Host "  Copy-Item path\to\best.pt $modelsDir\" -ForegroundColor Gray
    exit 1
}

Write-Host "Found best.pt, exporting to ONNX..." -ForegroundColor Green
Set-Location $modelsDir

python export_onnx.py --weights best.pt --output best.onnx --imgsz 640 --simplify --validate
if ($LASTEXITCODE -ne 0) {
    Write-Host "Export failed!" -ForegroundColor Red
    exit 1
}

if (Test-Path $onnxPath) {
    $size = (Get-Item $onnxPath).Length / 1MB
    Write-Host ""
    Write-Host "SUCCESS: best.onnx created ($([math]::Round($size, 2)) MB)" -ForegroundColor Green
    Write-Host "Detector can now start. Run: docker-compose up -d detector" -ForegroundColor Cyan
} else {
    Write-Host "ERROR: best.onnx was not created" -ForegroundColor Red
    exit 1
}
