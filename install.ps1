$ErrorActionPreference = "Stop"

$Repo       = "tonrakun/T0K3N-MCP"
$Artifact   = "t0k3n-mcp-windows-x86_64.exe"
$InstallDir = "$env:USERPROFILE\.local\bin"
$Url        = "https://github.com/$Repo/releases/latest/download/$Artifact"
$OutPath    = "$InstallDir\t0k3n-mcp.exe"

Write-Host "Downloading $Artifact..."
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Invoke-WebRequest -Uri $Url -OutFile $OutPath -UseBasicParsing

Write-Host "Installed: $OutPath"

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to PATH (restart your terminal to take effect)"
}
