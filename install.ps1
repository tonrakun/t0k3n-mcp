$ErrorActionPreference = "Stop"

$Repo        = "tonrakun/T0K3N-MCP"
$Artifact    = "t0k3n-mcp-windows-x86_64.exe"
$InstallDir  = "$env:USERPROFILE\t0k3n-mcp"
$BinPath     = "$InstallDir\t0k3n-mcp.exe"
$VersionFile = "$InstallDir\VERSION"
$TotalSteps  = 4

function Write-Step([int]$Num, [string]$Message) {
    Write-Host "[$Num/$TotalSteps] " -ForegroundColor Cyan -NoNewline
    Write-Host $Message
}
function Write-Ok([string]$Message)   { Write-Host "      OK  $Message" -ForegroundColor Green }
function Write-Info([string]$Message) { Write-Host "          $Message" -ForegroundColor DarkGray }
function Fail([string]$Message) {
    Write-Host "      NG  $Message" -ForegroundColor Red
    exit 1
}

# Run the binary with a timeout so a misbehaving exe can never hang the installer.
# stderr is swallowed: pre-2.5.0 binaries ignore --version and boot the server,
# which would otherwise spill its startup logs into the installer output.
function Get-BinaryVersion([string]$Path) {
    $stdout = [System.IO.Path]::GetTempFileName()
    $stderr = [System.IO.Path]::GetTempFileName()
    try {
        $proc = Start-Process -FilePath $Path -ArgumentList "--version" `
            -NoNewWindow -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        if (-not $proc.WaitForExit(5000)) {
            $proc.Kill()
            return $null
        }
        $out = (Get-Content $stdout -Raw -ErrorAction SilentlyContinue)
        if ($out -match "(\d+\.\d+\.\d+)") { return $Matches[1] }
        return $null
    } catch {
        return $null
    } finally {
        Remove-Item -Force $stdout, $stderr -ErrorAction SilentlyContinue
    }
}

$IsUpdate = Test-Path $BinPath
Write-Host ""
Write-Host ("t0k3n-mcp installer - " + ($(if ($IsUpdate) { "update" } else { "fresh install" }))) -ForegroundColor White
Write-Host ""

# ── 1. Resolve latest release ────────────────────────────────────────────────
Write-Step 1 "Checking latest release..."
$LatestVersion = $null
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" `
        -Headers @{ "User-Agent" = "t0k3n-mcp-installer" } -TimeoutSec 15
    $LatestVersion = $release.tag_name.TrimStart("v")
    Write-Ok "Latest release: v$LatestVersion"
} catch {
    Write-Info "GitHub API unavailable ($($_.Exception.Message)) — continuing without version check"
}

# ── 2. Check installed version ───────────────────────────────────────────────
Write-Step 2 "Checking installed version..."
$InstalledVersion = $null
if ($IsUpdate) {
    if (Test-Path $VersionFile) {
        $InstalledVersion = (Get-Content $VersionFile -Raw).Trim()
    } else {
        $InstalledVersion = Get-BinaryVersion $BinPath
    }
    if ($InstalledVersion) { Write-Ok "Installed: v$InstalledVersion" }
    else                   { Write-Info "Installed version unknown (pre-2.5.0 binary)" }

    if ($LatestVersion -and $InstalledVersion -eq $LatestVersion) {
        Write-Host ""
        Write-Host "Already up to date (v$InstalledVersion). Nothing to do." -ForegroundColor Green
        exit 0
    }
} else {
    Write-Ok "No existing install found"
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
}

# ── 3. Download ──────────────────────────────────────────────────────────────
$Url = if ($LatestVersion) {
    "https://github.com/$Repo/releases/download/v$LatestVersion/$Artifact"
} else {
    "https://github.com/$Repo/releases/latest/download/$Artifact"
}
$VersionLabel = if ($LatestVersion) { "v$LatestVersion" } else { "latest" }
Write-Step 3 "Downloading $VersionLabel..."
Write-Info $Url
$TmpPath = "$BinPath.new"
try {
    Invoke-WebRequest -Uri $Url -OutFile $TmpPath -UseBasicParsing
} catch {
    Fail "Download failed: $($_.Exception.Message)"
}
$size = (Get-Item $TmpPath).Length
if ($size -lt 1MB) {
    Remove-Item -Force $TmpPath -ErrorAction SilentlyContinue
    Fail "Downloaded file is too small ($size bytes) — not a valid binary"
}
Write-Ok ("Downloaded {0:N1} MB" -f ($size / 1MB))

# ── 4. Install / swap ────────────────────────────────────────────────────────
Write-Step 4 "Installing..."

# Clean up leftovers from previous updates (best effort — may still be locked)
Get-ChildItem "$InstallDir\*.old*" -ErrorAction SilentlyContinue | ForEach-Object {
    try { Remove-Item -Force $_.FullName -ErrorAction Stop } catch {}
}

if ($IsUpdate) {
    # A running MCP server locks the exe against deletion, but Windows allows
    # renaming a running exe — so swap by rename instead of delete + move.
    $OldPath = "$BinPath.old"
    if (Test-Path $OldPath) {
        $OldPath = "$BinPath.old-$(Get-Date -Format yyyyMMddHHmmss)"
    }
    try {
        Move-Item -Force -Path $BinPath -Destination $OldPath
    } catch {
        Remove-Item -Force $TmpPath -ErrorAction SilentlyContinue
        Fail "Could not move the existing binary aside: $($_.Exception.Message)"
    }
    Move-Item -Path $TmpPath -Destination $BinPath
    try {
        Remove-Item -Force $OldPath -ErrorAction Stop
    } catch {
        Write-Info "Old binary is still running; it will be cleaned up on the next update"
    }
} else {
    Move-Item -Path $TmpPath -Destination $BinPath
}

# Verify the new binary actually runs
$NewVersion = Get-BinaryVersion $BinPath
if ($NewVersion) {
    Set-Content -Path $VersionFile -Value $NewVersion -Encoding ASCII
    Write-Ok "Verified: t0k3n-mcp v$NewVersion"
} elseif ($LatestVersion) {
    Set-Content -Path $VersionFile -Value $LatestVersion -Encoding ASCII
    Write-Info "Binary installed (version probe unavailable on this release)"
} else {
    Write-Info "Binary installed"
}

# First-install extras: MCP config + PATH
if (-not $IsUpdate) {
    $Desktop        = [Environment]::GetFolderPath("Desktop")
    $BinPathEscaped = $BinPath -replace '\\', '\\\\'
    $McpJson        = @"
{
  "mcpServers": {
    "t0k3n": {
      "command": "$BinPathEscaped",
      "args": []
    }
  }
}
"@
    Set-Content -Path "$Desktop\.mcp.json" -Value $McpJson -Encoding UTF8
    Write-Ok "MCP config written: $Desktop\.mcp.json"

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        Write-Ok "Added $InstallDir to PATH (restart terminal to take effect)"
    }
}

Write-Host ""
if ($IsUpdate) {
    $fromLabel = if ($InstalledVersion) { "v$InstalledVersion" } else { "previous version" }
    $toLabel   = if ($NewVersion) { "v$NewVersion" } elseif ($LatestVersion) { "v$LatestVersion" } else { "latest" }
    Write-Host "Update complete: $fromLabel -> $toLabel" -ForegroundColor Green
    Write-Host "Restart Claude Code (or your MCP client) to load the new binary." -ForegroundColor White
} else {
    Write-Host "Install complete: $BinPath" -ForegroundColor Green
}
Write-Host ""
