param(
    [string]$RepoRoot = "$PSScriptRoot/..",
    [string]$TargetX = "X:/funny",
    [string]$TargetT = "T:/funny"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Push-Location $RepoRoot
try {
    Write-Host "Building release binary..."
    cargo build --release

    $bin = Join-Path $RepoRoot "target/release/funny.exe"
    if (-not (Test-Path $bin)) {
        throw "Binary not found: $bin"
    }

    foreach ($dest in @($TargetX, $TargetT)) {
        if (-not (Test-Path $dest)) {
            Write-Host "Creating destination folder $dest"
            New-Item -ItemType Directory -Path $dest -Force | Out-Null
        }
        $destFile = Join-Path $dest "funny.exe"
        Write-Host "Copying to $destFile"
        Copy-Item -Path $bin -Destination $destFile -Force
    }

    Write-Host "Done."
}
finally {
    Pop-Location
}
