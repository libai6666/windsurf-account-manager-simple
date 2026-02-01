@echo off
setlocal EnableDelayedExpansion
chcp 65001 >nul

cd /d "%~dp0\src-tauri"

:: 开发模式下禁用管理员权限要求
set "REQUIRE_ADMIN=false"

echo ========================================
echo   清理编译缓存并重新编译
echo ========================================
echo.
echo [信息] 正在清理编译缓存...

cargo clean

echo [信息] 清理完成！
echo [信息] 现在可以使用 dev.bat 启动开发环境了。
echo.
pause
