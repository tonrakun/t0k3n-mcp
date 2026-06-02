$ErrorActionPreference = "Stop"

$Repo       = "tonrakun/T0K3N-MCP"
$Artifact   = "t0k3n-mcp-windows-x86_64.exe"
$InstallDir = "$env:USERPROFILE\t0k3n-mcp"
$BinPath    = "$InstallDir\t0k3n-mcp.exe"
$Url        = "https://github.com/$Repo/releases/latest/download/$Artifact"

if (Test-Path $BinPath) {
    # Update: download new binary first, swap only on success
    Write-Host "Updating t0k3n-mcp..."
    $TmpPath = "$BinPath.new"
    Invoke-WebRequest -Uri $Url -OutFile $TmpPath -UseBasicParsing
    Remove-Item -Force $BinPath
    Move-Item -Path $TmpPath -Destination $BinPath
    Write-Host "Updated: $BinPath"
} else {
    # Install: create folder, download binary, write desktop config
    Write-Host "Installing t0k3n-mcp..."
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Invoke-WebRequest -Uri $Url -OutFile $BinPath -UseBasicParsing
    Write-Host "Installed: $BinPath"

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
    Write-Host "MCP config written: $Desktop\.mcp.json"

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        Write-Host "Added $InstallDir to PATH (restart terminal to take effect)"
    }
}
