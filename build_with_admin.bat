@echo off
echo Building Windsurf Account Manager...
echo.

:: 步骤0: 清理旧的构建缓存（确保前端资源正确嵌入）
echo [0/2] Cleaning old build cache...
if exist "dist" rmdir /s /q "dist"
if exist "src-tauri\target\release" rmdir /s /q "src-tauri\target\release"

:: 步骤1: 编译 Tauri 应用（前端由 tauri.conf.json 的 beforeBuildCommand 自动构建）
echo.
echo [1/2] Building Tauri application (frontend will be built automatically)...
call npm run tauri build

if %ERRORLEVEL% NEQ 0 (
    echo Build failed!
    pause
    exit /b 1
)

echo.
echo Build successful! Setting administrator privileges...
echo.

:: 设置管理员权限
powershell -ExecutionPolicy Bypass -File "set_admin_manifest.ps1" "src-tauri\target\release\windsurf-account-manager.exe"

echo.
echo Done! The executable now requires administrator privileges.
echo.
echo Output files:
echo - Executable: src-tauri\target\release\windsurf-account-manager.exe
echo - Installer: src-tauri\target\release\bundle\
echo.
pause
