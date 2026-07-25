# Thin bootstrap: download the binary, put it on PATH, done.
# Everything else (updates, .mcp.json) is handled by the binary itself:
#   t0k3n upgrade / t0k3n setup
$ErrorActionPreference = "Stop"

$Repo       = "tonrakun/t0k3n-mcp"
$Artifact   = "t0k3n-windows-x86_64.exe"
$InstallDir = "$env:USERPROFILE\t0k3n-mcp"
$BinPath    = "$InstallDir\t0k3n.exe"

Write-Host ""
Write-Host "Installing t0k3n..." -ForegroundColor White
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# Clean up leftovers from previous installs (best effort — may still be locked)
Get-ChildItem "$InstallDir\t0k3n*.old*", "$InstallDir\VERSION" -ErrorAction SilentlyContinue | ForEach-Object {
    try { Remove-Item -Force $_.FullName -ErrorAction Stop } catch {}
}

# Download
$Base = "https://github.com/$Repo/releases/latest/download"
$Url = "$Base/$Artifact"
Write-Host "  $Url" -ForegroundColor DarkGray
$TmpPath = "$BinPath.new"
Invoke-WebRequest -Uri $Url -OutFile $TmpPath -UseBasicParsing
$size = (Get-Item $TmpPath).Length
if ($size -lt 1MB) {
    Remove-Item -Force $TmpPath -ErrorAction SilentlyContinue
    throw "Downloaded file is too small ($size bytes) - not a valid binary"
}

# Verify against the published checksum manifest before putting it on PATH.
try {
    $Sums = (Invoke-WebRequest -Uri "$Base/SHA256SUMS.txt" -UseBasicParsing).Content
} catch {
    Remove-Item -Force $TmpPath -ErrorAction SilentlyContinue
    throw "Could not download $Base/SHA256SUMS.txt - refusing to install an unverified binary"
}
$Expected = $Sums -split "`n" | ForEach-Object {
    $parts = $_.Trim() -split '\s+', 2
    if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $Artifact) { $parts[0] }
} | Select-Object -First 1
if (-not $Expected) {
    Remove-Item -Force $TmpPath -ErrorAction SilentlyContinue
    throw "SHA256SUMS.txt does not list $Artifact - refusing to install an unverified binary"
}
$Actual = (Get-FileHash -Algorithm SHA256 $TmpPath).Hash.ToLower()
if ($Actual -ne $Expected.ToLower()) {
    Remove-Item -Force $TmpPath -ErrorAction SilentlyContinue
    throw "Checksum mismatch for $Artifact`n  expected: $Expected`n  actual:   $Actual"
}
Write-Host "  sha256 verified" -ForegroundColor DarkGray

# Swap: a running exe blocks deletion but allows renaming
if (Test-Path $BinPath) {
    Move-Item -Force $BinPath "$BinPath.old-$PID"
    try { Remove-Item -Force "$BinPath.old-$PID" -ErrorAction Stop } catch {}
}
Move-Item $TmpPath $BinPath

# Keep the legacy name working for existing .mcp.json configs
$LegacyPath = "$InstallDir\t0k3n-mcp.exe"
if (Test-Path $LegacyPath) {
    Move-Item -Force $LegacyPath "$LegacyPath.old-$PID"
    try { Remove-Item -Force "$LegacyPath.old-$PID" -ErrorAction Stop } catch {}
    Copy-Item $BinPath $LegacyPath
}

# User-level PATH — no elevation needed
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to PATH (restart terminal to take effect)" -ForegroundColor Green
}

$ver = & $BinPath version
Write-Host ""
Write-Host "Install complete: $ver" -ForegroundColor Green
Write-Host "  $BinPath"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  t0k3n setup    # write .mcp.json in your project directory"
Write-Host "  t0k3n upgrade  # update to the latest release"
Write-Host ""
