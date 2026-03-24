param(
    [Parameter(Mandatory=$true)]
    [string]$Token
)

$ErrorActionPreference = "Stop"

$REPO_URL    = "https://github.com/MrNMD2002/detectfire"
$RUNNER_DIR  = "C:\actions-runner"
$RUNNER_NAME = "fire-detection-runner"

Write-Host "=== GitHub Actions Self-hosted Runner Setup ===" -ForegroundColor Cyan

# Step 1: Get latest runner version
Write-Host "[1/5] Getting latest runner version..." -ForegroundColor Yellow
$latest  = Invoke-RestMethod -Uri "https://api.github.com/repos/actions/runner/releases/latest"
$version = $latest.tag_name.TrimStart("v")
$url     = "https://github.com/actions/runner/releases/download/v${version}/actions-runner-win-x64-${version}.zip"
Write-Host "      Version: v$version" -ForegroundColor Green

# Step 2: Create dir and download
Write-Host "[2/5] Downloading runner to $RUNNER_DIR ..." -ForegroundColor Yellow
if (Test-Path $RUNNER_DIR) {
    Remove-Item -Recurse -Force $RUNNER_DIR
}
New-Item -ItemType Directory -Path $RUNNER_DIR | Out-Null

$zip = "$RUNNER_DIR\runner.zip"
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory($zip, $RUNNER_DIR)
Remove-Item $zip
Write-Host "      Download complete." -ForegroundColor Green

# Step 3: Configure
Write-Host "[3/5] Configuring runner..." -ForegroundColor Yellow
Set-Location $RUNNER_DIR
& "$RUNNER_DIR\config.cmd" `
    --url $REPO_URL `
    --token $Token `
    --name $RUNNER_NAME `
    --work "_work" `
    --labels "self-hosted,Windows,x64,gpu" `
    --unattended `
    --replace
Write-Host "      Runner configured." -ForegroundColor Green

# Step 4: Install and start as Windows Service
Write-Host "[4/5] Installing as Windows Service..." -ForegroundColor Yellow
& "$RUNNER_DIR\svc.sh" install 2>$null
Start-Sleep -Seconds 2
$svc = Get-Service -Name "*actions.runner*" -ErrorAction SilentlyContinue
if ($svc) {
    Start-Service $svc.Name
    Write-Host "      Service started: $($svc.Name)" -ForegroundColor Green
} else {
    Write-Host "      Service not found - starting runner directly..." -ForegroundColor DarkYellow
    Start-Process -FilePath "$RUNNER_DIR\run.cmd" -WindowStyle Minimized
    Write-Host "      Runner started in background." -ForegroundColor Green
}

# Step 5: Verify
Write-Host "[5/5] Verifying..." -ForegroundColor Yellow
Start-Sleep -Seconds 3
$svc = Get-Service -Name "*actions.runner*" -ErrorAction SilentlyContinue
if ($svc) {
    Write-Host "      Service status: $($svc.Status)" -ForegroundColor Green
}

Write-Host ""
Write-Host "=== DONE - Runner is ready ===" -ForegroundColor Cyan
Write-Host "Check: https://github.com/MrNMD2002/detectfire/settings/actions/runners" -ForegroundColor White
Write-Host "Labels: self-hosted, Windows, x64, gpu" -ForegroundColor White
