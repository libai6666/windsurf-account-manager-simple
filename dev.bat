@echo off
setlocal EnableDelayedExpansion
chcp 65001 >nul

cd /d "%~dp0"

set "REQUIRE_ADMIN=false"

echo ========================================
echo   Windsurf Account Manager Dev
echo ========================================
echo.
echo [INFO] Dev mode, admin requirement disabled.
echo [INFO] If startup still asks for admin, run dev-clean.bat first.
echo.
echo [INFO] Starting...
echo.

npm run tauri dev
pause
