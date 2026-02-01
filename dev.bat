@echo off
setlocal EnableDelayedExpansion
chcp 65001 >nul

cd /d "%~dp0"

:: 开发模式下禁用管理员权限要求（通过环境变量控制build.rs）
set "REQUIRE_ADMIN=false"

echo ========================================
echo   Windsurf Account Manager 开发环境
echo ========================================
echo.
echo [信息] 开发模式（无需管理员权限）
echo [信息] 如果启动失败提示需要管理员权限，请先运行: dev-clean.bat
echo.
echo [信息] 正在启动...
echo.

npm run tauri dev
pause
