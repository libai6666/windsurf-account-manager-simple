@echo off
echo Building Windsurf Account Manager...
echo.

:: 步骤0: 清理旧的构建缓存（确保前端资源正确嵌入）
echo [0/3] Cleaning old build cache...
if exist "dist" rmdir /s /q "dist"
if exist "src-tauri\target\release" rmdir /s /q "src-tauri\target\release"

:: 步骤1: 先构建前端
echo.
echo [1/3] Building frontend...
call npm run build
if %ERRORLEVEL% NEQ 0 (
    echo Frontend build failed!
    pause
    exit /b 1
)

:: 验证前端构建结果
if not exist "dist\index.html" (
    echo ERROR: dist\index.html not found! Frontend build incomplete.
    pause
    exit /b 1
)

echo.
echo [2/3] Building Tauri application...
:: 步骤2: 编译 Tauri 应用
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
