# Tauri 桌面端开发调试说明

本文档说明如何在开发环境一键拉起 AgentDash Tauri 桌面端，以及常见排错方式。

## 前置准备

首次切到包含桌面端包的分支后，先安装 workspace 依赖：

```powershell
pnpm install
```

桌面端由两部分组成：

- `packages/app-tauri`：桌面 renderer，Vite dev server 固定使用 `127.0.0.1:5381`。
- `crates/agentdash-local-tauri`：Tauri v2 桌面壳。开发期默认复用独立 `agentdash-server`，打包/直接运行时仍可进程内托管 Dashboard API。
- `agentdash-server`：开发期独立后端进程，默认使用 `127.0.0.1:3001`。

## 一键启动

推荐使用根目录脚本：

```powershell
pnpm dev:desktop
```

脚本会按顺序执行：

1. 清理 `3001`、`5381`、`5382` 端口和残留 `agentdash-server` / `agentdash-local-tauri` 进程。
2. 先统一执行 `cargo build -p agentdash-api -p agentdash-local -p agentdash-local-tauri`，与 `pnpm dev` 使用同一套 dev Rust 编译目标。
3. 启动已构建的独立 `agentdash-server`。
4. 等待 `http://127.0.0.1:3001/api/health` 就绪。
5. 启动 `app-tauri` Vite dev server。
6. 等待 `http://127.0.0.1:5381` 就绪。
7. 启动 `agentdash-local-tauri` 桌面壳，并通过 `AGENTDASH_DESKTOP_API_MODE=external` 复用外部 `agentdash-server`。

窗口打开后，侧边栏包含两个入口：

- `Runtime`：本机 runtime 管理页，通过 Tauri command 访问 `agentdash-local` library。
- `Dashboard`：复用 Web Dashboard，开发期通过独立 `agentdash-server` 访问数据。

按 `Ctrl+C` 会停止独立后端、桌面前端和 Tauri 壳子进程。

## 与 `pnpm dev` 的关系

`pnpm dev` 仍然面向 Web 联合调试，入口是 `scripts/dev-joint.js`：

1. 清理云端后端、本机后端和 Web 前端相关端口。
2. 构建统一 dev Rust 目标：`agentdash-api`、`agentdash-local`、`agentdash-local-tauri`。
3. 启动云端后端 `agentdash-server`。
4. 确保并启动本机后端 `agentdash-local`。
5. 启动 Web 前端 `app-web`。

`pnpm dev:desktop` 面向 Tauri 桌面端调试，入口是 `scripts/dev-desktop.js`：

1. 清理独立后端、桌面 renderer 和残留 Tauri 壳。
2. 先统一编译 dev Rust 目标：`agentdash-api`、`agentdash-local`、`agentdash-local-tauri`，避免任一长驻进程编译期间就开始做健康探测。
3. 启动独立 `agentdash-server`，用于后端日志、断点和 API 调试。
4. 启动桌面 renderer `app-tauri`。
5. 启动 Tauri 壳 `agentdash-local-tauri`，并让它复用外部 `agentdash-server`。

直接执行 `pnpm run dev:desktop-shell` 时不会自动设置 external 模式；这种方式会走 Tauri 壳的默认行为，由壳进程内托管 Dashboard API。需要调试独立后端时优先使用 `pnpm dev:desktop`。

两个脚本都复用 `scripts/lib/dev-process.js` 中的开发进程基础设施：

- 子进程 supervisor。
- `Ctrl+C` 统一收尾。
- 子进程树停止。
- `runCommand`。
- HTTP ready 探测。
- JSON HTTP 请求。
- 按进程名清理残留进程树。

业务编排仍保留在各自入口中，避免把 Web 联合调试和桌面端调试揉成难以维护的通用流程。

因为两条入口共享同一套 Rust 编译目标，`pnpm dev` 在 build 前也会清理残留 `agentdash-local-tauri` 进程，避免 Windows 锁定 `target/debug/agentdash-local-tauri.exe` 导致编译失败。

## 常用参数

```powershell
pnpm dev:desktop -- --skip-clean
pnpm dev:desktop -- --skip-build
pnpm dev:desktop -- --skip-server
pnpm dev:desktop -- --skip-frontend
pnpm dev:desktop -- --skip-shell
```

- `--skip-clean`：保留现有端口和桌面壳进程，适合正在定位进程退出问题时使用。
- `--skip-build`：跳过 Rust 构建，直接启动已有 binary。
- `--skip-server`：复用已经启动的 `agentdash-server`。
- `--skip-frontend`：复用已经启动的 `app-tauri` Vite server。
- `--skip-shell`：只启动/检查桌面前端，不打开 Tauri 窗口。

## 分步调试

如果一键脚本失败，可以拆成三个终端排查。

终端 1：

```powershell
pnpm run dev:server
```

终端 2：

```powershell
pnpm run dev:desktop-frontend
```

终端 3：

```powershell
$env:AGENTDASH_DESKTOP_API_MODE = "external"
$env:AGENTDASH_DESKTOP_API_ORIGIN = "http://127.0.0.1:3001"
pnpm run dev:desktop-shell
```

健康检查：

```powershell
Invoke-WebRequest http://127.0.0.1:3001/api/health
```

返回 `200` 表示独立 `agentdash-server` 已启动。

## 验证命令

桌面端专项检查：

```powershell
pnpm run desktop:check
```

桌面 renderer 构建：

```powershell
pnpm --filter app-tauri build
```

生成 release exe：

```powershell
pnpm run desktop:build
```

生成 Windows NSIS 安装包：

```powershell
pnpm run desktop:bundle
```

预期安装包路径：

```text
target/release/bundle/nsis/AgentDash_0.1.0_x64-setup.exe
```

## 常见问题

### 端口占用

桌面端默认占用：

- `3001`：开发期独立 `agentdash-server`，或直接运行 Tauri 壳时由壳进程托管的 Dashboard API。
- `5381`：`app-tauri` Vite dev server。
- `5382`：`app-tauri` preview 端口。

手动清理：

```powershell
node scripts/kill-ports.js 3001 5381 5382
```

### Rust 更新后没有生效

Rust 后端和 Tauri 壳不能热重载。修改 `crates/agentdash-local-tauri`、`crates/agentdash-api` 或相关 Rust crate 后，需要停止 `pnpm dev:desktop`，再重新启动。

### Dashboard 一直停在 API 检查页

先检查桌面 API：

```powershell
Invoke-WebRequest http://127.0.0.1:3001/api/health
```

如果不通，通常是 `agentdash-server` 未启动，或 `3001` 被其它旧进程占用。停止旧的 `pnpm dev`、`pnpm run dev:backend` 或重新执行：

```powershell
pnpm dev:desktop
```

### TypeScript 找不到 workspace 包

如果出现 `Cannot find module 'react'`、`Cannot find module '@agentdash/core/local-runtime'`，先重新安装依赖：

```powershell
pnpm install
```
