[CmdletBinding()]
param(
[switch]$SkipTests,
[switch]$SkipUi,
[switch]$OpenOutput
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot   = Split-Path -Parent $PSScriptRoot
$UiDir      = Join-Path $RepoRoot 'src-ui'
$BackendDir = Join-Path $RepoRoot 'src-backend'
$IssPath    = Join-Path $RepoRoot 'installer\f-hub.iss'
$CargoToml  = Join-Path $BackendDir 'Cargo.toml'
$OutputDir  = Join-Path $RepoRoot 'dist-installer'

function Invoke-Step {
param(
    [Parameter(Mandatory)][string]$Title,
    [Parameter(Mandatory)][scriptblock]$Action
)
Write-Host ""
Write-Host "=== $Title ===" -ForegroundColor Cyan
$global:LASTEXITCODE = 0
& $Action
if ($LASTEXITCODE -ne 0) {
    throw "$Title failed with exit code $LASTEXITCODE."
}
}

function Find-Iscc {
$cmd = Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue
if ($cmd) {
    return $cmd.Source
}

$localApp = [Environment]::GetFolderPath('LocalApplicationData')
$progFiles86 = [Environment]::GetFolderPath('ProgramFilesX86')
$progFiles64 = [Environment]::GetFolderPath('ProgramFiles')

$candidates = @(
    (Join-Path $localApp 'Programs\Inno Setup 6\ISCC.exe'),
    (Join-Path $progFiles86 'Inno Setup 6\ISCC.exe'),
    (Join-Path $progFiles64 'Inno Setup 6\ISCC.exe')
)
foreach ($p in $candidates) {
    if ($p -and (Test-Path $p)) {
        return $p
    }
}
throw "ISCC.exe (Inno Setup 6) not found. Install via: winget install -e --id JRSoftware.InnoSetup"
}

function Get-CargoVersion {
param([string]$TomlPath)
$content = Get-Content -Path $TomlPath -Raw -Encoding UTF8
if ($content -match '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"') {
    return $Matches[1]
}
throw "Failed to read package version from $TomlPath."
}

$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()

try {
$version = Get-CargoVersion -TomlPath $CargoToml
$iscc    = Find-Iscc

Write-Host "Starting F/HUB release build (v$version)" -ForegroundColor Green
Write-Host "  Repo : $RepoRoot"
Write-Host "  ISCC : $iscc"

$releaseDir = Join-Path $BackendDir 'target\release'
$runningInTarget = Get-Process -Name 'f-hub', 'f-hub-tray' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path.StartsWith($releaseDir, [System.StringComparison]::OrdinalIgnoreCase) }
if ($runningInTarget) {
    Write-Host "Stopping running F/HUB processes from target\release..." -ForegroundColor Yellow
    $runningInTarget | Stop-Process -Force
    Start-Sleep -Milliseconds 500
}

if (-not $SkipUi) {
    Invoke-Step -Title '1/4 Frontend build (pnpm build)' -Action {
        Push-Location $UiDir
        try {
            pnpm build
        } finally {
            Pop-Location
        }
    }
} else {
    Write-Host ""
    Write-Host "=== 1/4 Frontend build (skipped) ===" -ForegroundColor DarkGray
}

if (-not $SkipTests) {
    Invoke-Step -Title '2/4 Backend test (cargo test)' -Action {
        Push-Location $BackendDir
        try {
            cargo test
        } finally {
            Pop-Location
        }
    }
} else {
    Write-Host ""
    Write-Host "=== 2/4 Backend test (skipped) ===" -ForegroundColor DarkGray
}

Invoke-Step -Title '3/4 Backend release build (cargo build --release)' -Action {
    Push-Location $BackendDir
    try {
        cargo build --release
    } finally {
        Pop-Location
    }
}

Invoke-Step -Title "4/4 Installer build (ISCC.exe / v$version)" -Action {
    & $iscc "/DMyAppVersion=$version" $IssPath
}

$setupExe = Join-Path $OutputDir "F-HUB-Setup-$version.exe"
$stopwatch.Stop()

Write-Host ""
Write-Host "Release build completed ($($stopwatch.Elapsed.ToString('mm\:ss')))" -ForegroundColor Green
if (Test-Path $setupExe) {
    $sizeMb = [math]::Round((Get-Item $setupExe).Length / 1MB, 2)
    Write-Host "  Output: $setupExe ($sizeMb MB)" -ForegroundColor Green
}

if ($OpenOutput -and (Test-Path $OutputDir)) {
    Start-Process explorer.exe $OutputDir
}
}
catch {
Write-Host ""
Write-Host "Build failed: $($_.Exception.Message)" -ForegroundColor Red
exit 1
}