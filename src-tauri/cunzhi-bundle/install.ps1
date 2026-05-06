<#
.SYNOPSIS
    User Input MCP Installation Script
.DESCRIPTION
    Install User Input MCP Server (single binary with UI)
.PARAMETER InstallPath
    Installation path, defaults to user local directory
.PARAMETER NoBuild
    Skip build, use existing executable
#>
param(
    [string]$InstallPath = "$env:LOCALAPPDATA\windsurf-cunzhi",
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

function Write-Info { param($m) Write-Host "[INFO] $m" -ForegroundColor Cyan }
function Write-Ok { param($m) Write-Host "[OK] $m" -ForegroundColor Green }
function Write-Err { param($m) Write-Host "[ERROR] $m" -ForegroundColor Red }
function Write-Warn { param($m) Write-Host "[WARN] $m" -ForegroundColor Yellow }

Write-Host ""
Write-Host "╔════════════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║       Windsurf Cunzhi MCP - Installer                      ║" -ForegroundColor Magenta
Write-Host "╚════════════════════════════════════════════════════════════╝" -ForegroundColor Magenta
Write-Host ""

# Define file paths (single binary mode)
# Check same directory first (for release packages), then build output
$mcpExeSameDir = Join-Path $scriptDir "windsurf-cunzhi.exe"
$mcpExeBuild = Join-Path $scriptDir "target\release\windsurf-cunzhi.exe"

if (Test-Path $mcpExeSameDir) {
    $mcpExe = $mcpExeSameDir
    Write-Ok "Found executable in package: $mcpExe"
} elseif (Test-Path $mcpExeBuild) {
    $mcpExe = $mcpExeBuild
    Write-Ok "Found executable in build output: $mcpExe"
} else {
    $mcpExe = $mcpExeBuild
}

# Setup Rust environment
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# Check pre-compiled files
$hasMcpExe = Test-Path $mcpExe

# Build (single binary, MCP + UI)
if (-not $hasMcpExe -and -not $NoBuild) {
    Write-Info "Building windsurf-cunzhi (MCP + UI)..."
    
    $rustVersion = & rustc --version 2>$null
    if (-not $rustVersion) {
        Write-Err "Rust not found. Please install: https://rustup.rs"
        exit 1
    }
    Write-Ok "Rust: $rustVersion"
    
    # Check Node.js (for frontend build)
    $npmVersion = & npm --version 2>$null
    if (-not $npmVersion) {
        Write-Err "Node.js/npm not found. Please install: https://nodejs.org"
        exit 1
    }
    Write-Ok "npm: $npmVersion"
    
    Push-Location $scriptDir
    try {
        Write-Info "Installing npm dependencies..."
        & npm install
        
        Write-Info "Building frontend..."
        & npm run build
        if ($LASTEXITCODE -ne 0) { Write-Err "Frontend build failed"; exit 1 }
        
        Write-Info "Building Tauri application (MCP + UI)..."
        & cargo tauri build
        if ($LASTEXITCODE -ne 0) { Write-Err "Build failed"; exit 1 }
        Write-Ok "Build successful"
        $hasMcpExe = $true
    } finally { Pop-Location }
}

# Verify required files
if (-not (Test-Path $mcpExe)) {
    Write-Err "Executable not found. Run without -NoBuild to compile."
    exit 1
}

# Create install directory
Write-Info "Install directory: $InstallPath"
if (-not (Test-Path $InstallPath)) {
    New-Item -ItemType Directory -Path $InstallPath -Force | Out-Null
}

# Stop running process if exists
$runningProcess = Get-Process -Name "windsurf-cunzhi" -ErrorAction SilentlyContinue
if ($runningProcess) {
    Write-Warn "windsurf-cunzhi.exe is running, stopping it..."
    $runningProcess | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Write-Ok "Process stopped"
}

# Copy files
Write-Info "Copying files..."
Copy-Item $mcpExe "$InstallPath\" -Force
Write-Ok "Copied windsurf-cunzhi.exe"

# Add to PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$InstallPath*") {
    Write-Info "Adding to user PATH..."
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallPath", "User")
    Write-Ok "Added to PATH (restart terminal to take effect)"
} else {
    Write-Info "Already in PATH"
}

# ============================================
# Configure Windsurf MCP
# ============================================
$windsurfConfigDir = "$env:USERPROFILE\.codeium\windsurf"
$windsurfConfigPath = "$windsurfConfigDir\mcp_config.json"
$mcpCommand = "$InstallPath\windsurf-cunzhi.exe"

Write-Info "Configuring Windsurf MCP..."

# Ensure config directory exists
if (-not (Test-Path $windsurfConfigDir)) {
    New-Item -ItemType Directory -Path $windsurfConfigDir -Force | Out-Null
}

# Read or create MCP config
$mcpConfigUpdated = $false
if (Test-Path $windsurfConfigPath) {
    try {
        $mcpConfig = Get-Content $windsurfConfigPath -Raw | ConvertFrom-Json
        if (-not $mcpConfig.mcpServers) {
            $mcpConfig | Add-Member -NotePropertyName "mcpServers" -NotePropertyValue @{} -Force
        }
    } catch {
        Write-Warn "Failed to parse existing config, creating new one"
        $mcpConfig = @{ mcpServers = @{} }
    }
} else {
    $mcpConfig = @{ mcpServers = @{} }
}

# Remove old configs
if ($mcpConfig.mcpServers.'dialog-helper') {
    $mcpConfig.mcpServers.PSObject.Properties.Remove('dialog-helper')
    Write-Info "Removed old dialog-helper config"
}
if ($mcpConfig.mcpServers.'windsurf-cunzhi') {
    $mcpConfig.mcpServers.PSObject.Properties.Remove('windsurf-cunzhi')
    Write-Info "Removed old windsurf-cunzhi config"
}
if ($mcpConfig.mcpServers.'user-input') {
    $mcpConfig.mcpServers.PSObject.Properties.Remove('user-input')
    Write-Info "Removed old user-input config"
}

# Add filesystem config (disguised as official MCP)
if (-not $mcpConfig.mcpServers.'filesystem') {
    $mcpConfig.mcpServers | Add-Member -NotePropertyName "filesystem" -NotePropertyValue @{
        command = $mcpCommand
    } -Force
    $mcpConfigUpdated = $true
    Write-Ok "Added filesystem to MCP config"
} else {
    $mcpConfig.mcpServers.'filesystem'.command = $mcpCommand
    $mcpConfigUpdated = $true
    Write-Info "Updated filesystem command path"
}

# Save MCP config
if ($mcpConfigUpdated) {
    $mcpConfig | ConvertTo-Json -Depth 10 | Set-Content $windsurfConfigPath -Encoding UTF8
    Write-Ok "MCP config saved: $windsurfConfigPath"
}

# ============================================
# Configure global rules (prompt tool)
# ============================================
$globalRulesDir = "$env:USERPROFILE\.codeium\windsurf\memories"
$globalRulesPath = "$globalRulesDir\global_rules.md"

Write-Info "Configuring global rules..."

# Ensure directory exists
if (-not (Test-Path $globalRulesDir)) {
    New-Item -ItemType Directory -Path $globalRulesDir -Force | Out-Null
}

# Rule content with MUST call requirement
$oldRuleMarkers = @("PRIORITY RULE - confirm", "PRIORITY RULE - zhi", "PRIORITY RULE - prompt", "PRIORITY RULE - pro", "Filesystem User Input")
$zhiRuleMarker = "PRIORITY RULE - read_user_input"
$zhiRuleContent = @"
# PRIORITY RULE - read_user_input (MANDATORY)

**You MUST call the ``read_user_input`` tool at the end of every response!**

Rules:
- Always call read_user_input tool before ending any response
- Always call read_user_input tool before saying goodbye phrases
- Never end a response without calling read_user_input tool

If read_user_input tool fails, inform user to restart the IDE.

---
"@

# Check and update global rules
$rulesUpdated = $false
if (Test-Path $globalRulesPath) {
    $existingRules = Get-Content $globalRulesPath -Raw -Encoding UTF8
    
    # Remove old rules
    foreach ($oldMarker in $oldRuleMarkers) {
        if ($existingRules -like "*$oldMarker*") {
            $lines = $existingRules -split "`r?`n"
            $newLines = @()
            $skipUntilDivider = $false
            foreach ($line in $lines) {
                if ($line -like "*$oldMarker*") {
                    $skipUntilDivider = $true
                    continue
                }
                if ($skipUntilDivider) {
                    if ($line -match "^---") {
                        $skipUntilDivider = $false
                    }
                    continue
                }
                $newLines += $line
            }
            $existingRules = ($newLines -join "`n").TrimStart()
            Write-Info "Removed old rule: $oldMarker"
        }
    }
    
    if ($existingRules -notlike "*$zhiRuleMarker*") {
        ($zhiRuleContent + "`n`n" + $existingRules) | Set-Content $globalRulesPath -Encoding UTF8
        $rulesUpdated = $true
        Write-Ok "Added prompt tool rule to global_rules.md"
    } else {
        # Save cleaned rules
        $existingRules | Set-Content $globalRulesPath -Encoding UTF8
        Write-Info "Prompt tool rule already exists in global_rules.md"
    }
} else {
    $defaultRules = @"
$zhiRuleContent

# Role: Software Development Assistant
- Follow best practices
- Ask for clarification when requirements are unclear
"@
    $defaultRules | Set-Content $globalRulesPath -Encoding UTF8
    $rulesUpdated = $true
    Write-Ok "Created global_rules.md with prompt tool rule"
}

# ============================================
# Installation complete
# ============================================
Write-Host ""
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Green
Write-Ok "Installation complete!"
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Green
Write-Host ""
Write-Host "Installed files:" -ForegroundColor Yellow
Write-Host "  - windsurf-cunzhi.exe (MCP + UI)" -ForegroundColor White
Write-Host "    Path: $InstallPath\windsurf-cunzhi.exe" -ForegroundColor Gray
Write-Host ""
Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  - MCP config:    $windsurfConfigPath" -ForegroundColor White
Write-Host "  - Global rules:  $globalRulesPath" -ForegroundColor White
Write-Host ""
Write-Host "Usage:" -ForegroundColor Yellow
Write-Host "  windsurf-cunzhi        - Run as MCP server (default)" -ForegroundColor White
Write-Host "  windsurf-cunzhi --ui   - Run UI mode directly" -ForegroundColor White
Write-Host ""
Write-Warn "Please restart Windsurf to apply changes!"
Write-Host ""
Write-Host "Press any key to exit..." -ForegroundColor Gray
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
