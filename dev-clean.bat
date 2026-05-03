@echo off
setlocal EnableDelayedExpansion
chcp 65001 >nul

cd /d "%~dp0\src-tauri"

set "REQUIRE_ADMIN=false"

echo ========================================
echo   Clean Build Cache
echo ========================================
echo.
echo [INFO] Cleaning build cache...

cargo clean

echo [INFO] Clean finished.
echo [INFO] You can now start with dev.bat.
echo.
pause
