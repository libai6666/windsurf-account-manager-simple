# Windsurf Account Manager 开发调试指南

## 目录

- [环境要求](#环境要求)
- [项目结构](#项目结构)
- [开发调试](#开发调试)
  - [方式一：完整开发模式](#方式一完整开发模式)
  - [方式二：分离调试模式](#方式二分离调试模式)
- [构建正式包](#构建正式包)
- [常见问题](#常见问题)

---

## 环境要求

### 必需软件

1. **Node.js** (v18+)
   - 下载地址: https://nodejs.org/
   - 验证安装: `node -v` 和 `npm -v`

2. **Rust** (最新稳定版)
   - 下载地址: https://rustup.rs/
   - 验证安装: `rustc --version` 和 `cargo --version`

3. **Visual Studio Build Tools** (Windows)
   - 需要安装 "C++ 桌面开发" 工作负载
   - 需要安装 Windows SDK (如 Windows 11 SDK 22621)

### 可选软件

- **VS Code** 或 **Cursor** IDE
- **Rust Analyzer** 插件 (用于 Rust 代码提示)
- **Volar** 插件 (用于 Vue 开发)

---

## 项目结构

```
windsurf-account-manager-simple/
├── src/                    # Vue 前端源码
│   ├── api/               # API 接口定义
│   ├── components/        # Vue 组件
│   ├── store/             # Pinia 状态管理
│   ├── views/             # 页面视图
│   └── utils/             # 工具函数
├── src-tauri/             # Rust 后端源码
│   ├── src/
│   │   ├── commands/      # Tauri 命令
│   │   ├── models/        # 数据模型
│   │   ├── services/      # 业务服务
│   │   └── utils/         # 工具函数
│   ├── Cargo.toml         # Rust 依赖配置
│   └── tauri.conf.json    # Tauri 配置
├── package.json           # Node.js 依赖配置
└── vite.config.ts         # Vite 构建配置
```

---

## 开发调试

### 方式一：完整开发模式

这是最简单的开发方式，一条命令启动前端和后端。

#### 步骤

1. **安装依赖**（首次或依赖变更后执行）
   ```powershell
   cd d:\CursorWork\windsurf-account-manager-simple
   npm install
   ```

2. **启动开发服务器**

   ```powershell
   npm run tauri dev
   ```
备注：Stop-Process -Name "node" -Force -ErrorAction  停止之前的服务器的命令
   > ⚠️ **注意**：此命令需要**管理员权限**运行。如果出现权限错误 (os error 740)，请以管理员身份打开终端。

3. **等待编译完成**
   - 首次编译 Rust 后端可能需要 2-5 分钟
   - 后续增量编译会很快（几秒到几十秒）

4. **应用窗口自动打开**
   - 前端代码修改会自动热更新
   - 后端代码修改会自动重新编译并重启

---

### 方式二：分离调试模式

适合需要分别调试前端和后端的场景。

#### 步骤 1：启动前端开发服务器

```powershell
cd d:\CursorWork\windsurf-account-manager-simple
npm run dev
```

输出示例：
```
VITE v6.4.1  ready in 263 ms

➜  Local:   http://localhost:46952/
```

#### 步骤 2：编译并运行后端

**方法 A：使用 Debug 版本（快速编译，适合调试）**

```powershell
cd d:\CursorWork\windsurf-account-manager-simple\src-tauri
cargo build
```

然后以管理员权限运行：
```powershell
Start-Process -FilePath "D:\CursorWork\windsurf-account-manager-simple\src-tauri\target\debug\windsurf-account-manager.exe" -Verb RunAs
```

**方法 B：使用 Release 版本（优化编译，运行更快）**

```powershell
cd d:\CursorWork\windsurf-account-manager-simple\src-tauri
cargo build --release
```

然后以管理员权限运行：
```powershell
Start-Process -FilePath "D:\CursorWork\windsurf-account-manager-simple\src-tauri\target\release\windsurf-account-manager.exe" -Verb RunAs
```

#### 步骤 3：在浏览器中调试前端（可选）

打开浏览器访问 `http://localhost:46952/` 可以直接调试前端界面（但无法使用 Tauri 后端功能）。

---

## 构建正式包

### 构建步骤

1. **确保代码没有错误**
   ```powershell
   cd d:\CursorWork\windsurf-account-manager-simple
   npm run build
   ```

2. **构建 Tauri 应用**
   ```powershell
   npm run tauri build
   ```

3. **等待构建完成**
   - 构建过程包括：
     - Vue 前端打包
     - Rust 后端 Release 编译
     - 生成安装包（MSI/NSIS）

### 构建产物位置

构建完成后，文件位于：

| 文件类型 | 路径 |
|---------|------|
| 可执行文件 | `src-tauri/target/release/windsurf-account-manager.exe` |
| MSI 安装包 | `src-tauri/target/release/bundle/msi/` |
| NSIS 安装包 | `src-tauri/target/release/bundle/nsis/` |

### 直接运行 Release 版本

如果只需要 exe 文件，可以直接使用：
```powershell
Start-Process -FilePath "D:\CursorWork\windsurf-account-manager-simple\src-tauri\target\release\windsurf-account-manager.exe" -Verb RunAs
```

---

## 常见问题

### 1. 端口被占用

**错误信息**：`Error: Port 46952 is already in use`

**解决方案**：
```powershell
# 停止所有 Node 进程
Stop-Process -Name "node" -Force -ErrorAction SilentlyContinue

# 或查找占用端口的进程并关闭
netstat -ano | findstr :46952
taskkill /PID <进程ID> /F
```

### 2. 权限不足

**错误信息**：`请求的操作需要提升。(os error 740)`

**解决方案**：以管理员身份运行 PowerShell 或终端，或使用 `Start-Process -Verb RunAs`。

### 3. Rust 编译错误

**错误信息**：`linker link.exe not found` 或 `LNK1181: 无法打开输入文件 kernel32.lib`

**解决方案**：
1. 安装 Visual Studio Build Tools
2. 选择 "C++ 桌面开发" 工作负载
3. 安装 Windows SDK

### 4. 前端页面空白

**现象**：应用打开后显示 "无法访问此页面" 或空白

**原因**：前端开发服务器未运行

**解决方案**：
1. 确保 `npm run dev` 正在运行
2. 或使用 Release 版本（已打包前端）

### 5. 打包超时

**错误信息**：`timeout: global` 或 MSI/NSIS 打包失败

**解决方案**：
- 这通常是网络问题导致下载 WiX 工具包超时
- 可以直接使用 `src-tauri/target/release/windsurf-account-manager.exe`
- 或手动下载 WiX 并配置环境变量

---

## 快速命令参考

| 操作 | 命令 |
|------|------|
| 安装依赖 | `npm install` |
| 启动完整开发模式 | `npm run tauri dev` |
| 仅启动前端服务器 | `npm run dev` |
| 编译 Debug 版本 | `cd src-tauri && cargo build` |
| 编译 Release 版本 | `cd src-tauri && cargo build --release` |
| 构建正式安装包 | `npm run tauri build` |
| 运行 Release 版本 | `Start-Process ".\src-tauri\target\release\windsurf-account-manager.exe" -Verb RunAs` |

---

## 更新日志

- **2026-01-31**: 添加批量获取试用链接功能
  - 支持多账号批量验证和获取
  - 每个链接在独立浏览器窗口打开
  - 验证成功后自动触发获取链接
