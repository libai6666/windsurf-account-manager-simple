# Tauri 项目 GitHub Actions CI/CD 配置指南

> 本文档介绍如何使用 GitHub Actions 为本项目构建多平台安装包（重点：macOS）。

---

## 目录

- [前置条件](#前置条件)
- [项目已有配置说明](#项目已有配置说明)
- [快速开始：3步打出 macOS 包](#快速开始3步打出-macos-包)
- [工作流详解](#工作流详解)
- [仅构建 macOS 包（精简版）](#仅构建-macos-包精简版)
- [macOS 代码签名（可选）](#macos-代码签名可选)
- [常见问题](#常见问题)

---

## 前置条件

1. **GitHub 仓库** — 项目代码已推送到 GitHub
2. **GitHub Actions** — 公开仓库免费；私有仓库每月有 2000 分钟免费额度
3. **代码签名**（可选） — 如果需要分发给其他用户，建议配置 Apple 开发者证书签名

---

## 项目已有配置说明

项目已包含完整的多平台构建工作流：

```
.github/workflows/build.yml
```

该工作流支持以下 **6 个平台**的构建：

| 平台 | Runner | 目标 | 产物 |
|------|--------|------|------|
| Windows x64 | `windows-latest` | `x86_64-pc-windows-msvc` | `.msi` / `.exe` |
| Windows ARM64 | `windows-latest` | `aarch64-pc-windows-msvc` | `.msi` / `.exe` |
| **macOS x64 (Intel)** | `macos-latest` | `x86_64-apple-darwin` | **`.dmg`** / `.app` |
| **macOS ARM64 (Apple Silicon)** | `macos-latest` | `aarch64-apple-darwin` | **`.dmg`** / `.app` |
| Linux x64 | `ubuntu-22.04` | 默认 | `.deb` / `.rpm` / `.AppImage` |
| Linux ARM64 | `ubuntu-22.04` | `aarch64-unknown-linux-gnu` | `.deb` / `.rpm` |

---

## 快速开始：3步打出 macOS 包

### 第 1 步：推送代码到 GitHub

```bash
# 如果还没有远程仓库，先创建
git remote add origin https://github.com/<你的用户名>/<仓库名>.git

# 推送代码（确保 .github/workflows/build.yml 包含在内）
git add .
git commit -m "Add CI/CD workflow"
git push -u origin main
```

### 第 2 步：手动触发构建

工作流配置了 `workflow_dispatch` 触发器，可以手动启动：

1. 打开 GitHub 仓库页面
2. 点击 **Actions** 标签
3. 左侧选择 **Build & Release** 工作流
4. 点击右上角 **Run workflow** 按钮
5. 选择分支，点击 **Run workflow** 确认

![手动触发示意](https://docs.github.com/assets/cb-39629/mw-1440/images/help/actions/actions-workflow-dispatch.webp)

### 第 3 步：下载构建产物

构建完成后（约 15-30 分钟）：

1. 进入该次 workflow run 的详情页
2. 页面底部 **Artifacts** 区域会列出所有平台的安装包
3. 下载 **macos-x64**（Intel Mac）或 **macos-arm64**（M1/M2/M3 Mac）
4. 将 `.dmg` 文件发给 Mac 用户即可安装

---

## 工作流详解

### 构建流程

```
get-version          → 从 package.json 读取版本号
    ↓
build-windows-x64   ─┐
build-windows-arm64  │
build-macos-x64      ├→ 并行构建 6 个平台
build-macos-arm64    │
build-linux-x64      │
build-linux-arm64   ─┘
    ↓
release              → 汇总所有产物，创建 GitHub Release（Draft）
```

### macOS 构建核心步骤

```yaml
build-macos-arm64:
  runs-on: macos-latest        # GitHub 提供的 macOS runner
  steps:
    - uses: actions/checkout@v4

    - uses: actions/setup-node@v4
      with:
        node-version: 20
        cache: 'npm'

    - uses: dtolnay/rust-toolchain@stable
      with:
        targets: aarch64-apple-darwin   # Apple Silicon 目标

    - uses: swatinem/rust-cache@v2      # Rust 编译缓存，加速后续构建
      with:
        workspaces: './src-tauri -> target'

    - run: npm ci                        # 安装前端依赖

    - uses: tauri-apps/tauri-action@v0   # Tauri 官方构建 Action
      env:
        GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
      with:
        args: --target aarch64-apple-darwin
```

### 关键说明

- **`tauri-apps/tauri-action@v0`** — Tauri 官方 Action，自动执行前端构建 + Rust 编译 + 打包
- **`swatinem/rust-cache@v2`** — 缓存 Rust 编译产物，首次构建约 20 分钟，后续约 5-10 分钟
- **`cunzhi` 平台文件** — `build.rs` 会根据目标平台自动从 `src-tauri/cunzhi/` 复制对应文件到 `cunzhi-bundle/`
- **产物路径** — macOS 的 `.dmg` 和 `.app` 位于 `src-tauri/target/<target>/release/bundle/`

---

## 仅构建 macOS 包（精简版）

如果你只需要 macOS 包，不想等所有 6 个平台都构建完，可以创建一个精简工作流：

创建文件 `.github/workflows/build-macos-only.yml`：

```yaml
name: Build macOS Only

on:
  workflow_dispatch:
    inputs:
      arch:
        description: '目标架构'
        required: true
        default: 'both'
        type: choice
        options:
          - both
          - arm64
          - x64

jobs:
  build-macos-arm64:
    if: inputs.arch == 'both' || inputs.arch == 'arm64'
    runs-on: macos-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'npm'

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin

      - name: Rust cache
        uses: swatinem/rust-cache@v2
        with:
          workspaces: './src-tauri -> target'

      - name: Install dependencies
        run: npm ci

      - name: Build Tauri App (macOS ARM64)
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          args: --target aarch64-apple-darwin

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: macos-arm64
          path: |
            src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/*.dmg
            src-tauri/target/aarch64-apple-darwin/release/bundle/macos/*.app
          if-no-files-found: warn

  build-macos-x64:
    if: inputs.arch == 'both' || inputs.arch == 'x64'
    runs-on: macos-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'npm'

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-apple-darwin

      - name: Rust cache
        uses: swatinem/rust-cache@v2
        with:
          workspaces: './src-tauri -> target'

      - name: Install dependencies
        run: npm ci

      - name: Build Tauri App (macOS x64)
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          args: --target x86_64-apple-darwin

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: macos-x64
          path: |
            src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/*.dmg
            src-tauri/target/x86_64-apple-darwin/release/bundle/macos/*.app
          if-no-files-found: warn
```

使用方法：Actions → Build macOS Only → Run workflow → 选择 `arm64` / `x64` / `both`。

---

## macOS 代码签名（可选）

未签名的 macOS 应用会触发 **"无法打开，因为无法验证开发者"** 的安全提示。用户可以通过 `右键 → 打开` 绕过，但如果你需要正式分发，建议配置签名。

### 无签名时用户的安装方式

告知 Mac 用户：
1. 下载 `.dmg` 文件并双击挂载
2. 将 `.app` 拖入 `应用程序` 文件夹
3. **首次打开时**：右键点击应用图标 → 选择 "打开" → 弹窗中点击 "打开"
4. 或者在终端执行：
   ```bash
   xattr -cr /Applications/windsurf-account-manager.app
   ```

### 配置 Apple 签名（需要 Apple 开发者账号，$99/年）

1. 从 Apple Developer 导出 `.p12` 证书和 provisioning profile
2. 在 GitHub 仓库的 **Settings → Secrets and variables → Actions** 中添加：

   | Secret 名称 | 值 |
   |---|---|
   | `APPLE_CERTIFICATE` | `.p12` 证书的 Base64 编码 |
   | `APPLE_CERTIFICATE_PASSWORD` | 证书密码 |
   | `APPLE_SIGNING_IDENTITY` | 签名身份，如 `Developer ID Application: Your Name (TEAM_ID)` |
   | `APPLE_ID` | Apple ID 邮箱 |
   | `APPLE_PASSWORD` | App 专用密码（在 appleid.apple.com 生成） |
   | `APPLE_TEAM_ID` | 开发者团队 ID |

3. 修改 macOS 构建步骤，添加签名环境变量：

   ```yaml
   - name: Build Tauri App (macOS ARM64)
     uses: tauri-apps/tauri-action@v0
     env:
       GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
       APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
       APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
       APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
       APPLE_ID: ${{ secrets.APPLE_ID }}
       APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
       APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
     with:
       args: --target aarch64-apple-darwin
   ```

   `tauri-apps/tauri-action` 检测到这些环境变量后会自动完成签名和公证（notarization）。

---

## 常见问题

### Q: 构建失败提示 `npm ci` 报错？

确保 `package-lock.json` 已提交到仓库。`npm ci` 依赖此文件，如果缺失会报错。

```bash
# 生成并提交 lock 文件
npm install
git add package-lock.json
git commit -m "Add package-lock.json"
git push
```

### Q: macOS 构建时 Rust 编译失败？

检查 `Cargo.toml` 中的依赖是否都支持 macOS。项目已配置了平台条件依赖：
- `winreg` / `winapi` — 仅 Windows（`[target.'cfg(windows)'.dependencies]`）
- `libc` — 仅 Unix（`[target.'cfg(unix)'.dependencies]`）

### Q: 构建产物太大下载慢？

GitHub Artifacts 默认保存 90 天，下载速度取决于网络。可以在 workflow 中调整保留天数：

```yaml
- uses: actions/upload-artifact@v4
  with:
    name: macos-arm64
    path: ...
    retention-days: 7    # 只保留 7 天
```

### Q: 如何自动在 push tag 时触发构建？

在 `build.yml` 的 `on` 部分添加：

```yaml
on:
  workflow_dispatch:
  push:
    tags:
      - 'v*'    # 匹配 v1.0.0, v1.7.0 等标签
```

然后通过打 tag 触发：

```bash
git tag v1.7.0
git push origin v1.7.0
```

### Q: 如何区分 Intel Mac 和 Apple Silicon Mac？

| 芯片 | 目标 | 产物名 |
|------|------|--------|
| Intel (2020年前的Mac) | `x86_64-apple-darwin` | `macos-x64` |
| Apple Silicon (M1/M2/M3/M4) | `aarch64-apple-darwin` | `macos-arm64` |

大多数现代 Mac 都是 Apple Silicon，优先提供 `arm64` 版本即可。Intel 用户也可以通过 Rosetta 2 运行 ARM 版本（但原生版性能更好）。

### Q: GitHub Actions 免费额度？

- **公开仓库**：完全免费，无限制
- **私有仓库**：每月 2000 分钟（Free 计划），macOS runner 按 **10 倍**计费
  - 即一次 macOS 构建 20 分钟 = 消耗 200 分钟额度
  - 建议将仓库设为公开，或购买更多额度

---

## 总结

| 操作 | 步骤 |
|------|------|
| **首次配置** | 推送代码（含 `.github/workflows/build.yml`）到 GitHub |
| **构建所有平台** | Actions → Build & Release → Run workflow |
| **仅构建 macOS** | 创建 `build-macos-only.yml` 后同上 |
| **下载 macOS 包** | 构建完成后在 Artifacts 下载 `macos-arm64` 或 `macos-x64` |
| **发给用户** | 将 `.dmg` 文件发给 Mac 用户，首次需右键 → 打开 |
