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

1. 从 `assets/brand/app-icon.svg` 生成 Web favicon 与 Tauri 桌面图标资源。
2. 清理 `3001`、`5381`、`5382` 端口和残留 `agentdash-server` / `agentdash-local-tauri` 进程。
3. 先统一执行 `cargo build -p agentdash-api -p agentdash-local -p agentdash-local-tauri`，与 `pnpm dev` 使用同一套 dev Rust 编译目标。
4. 启动已构建的独立 `agentdash-server`。
5. 等待 `http://127.0.0.1:3001/api/health` 就绪。
6. 启动 `app-tauri` Vite dev server。
7. 等待 `http://127.0.0.1:5381` 就绪。
8. 启动 `agentdash-local-tauri` 桌面壳，并通过 `AGENTDASH_DESKTOP_API_MODE=external` 复用外部 `agentdash-server`。

窗口打开后会直接进入复用 Web Dashboard 的主应用体验。本机 runtime 管理能力不再作为桌面端顶层入口存在，而是出现在 Web 设置页的 desktop-only `本机运行时` scope 中，通过 Tauri command 访问 `agentdash-local` library。

按 `Ctrl+C` 会停止独立后端、桌面前端和 Tauri 壳子进程。

## 与 `pnpm dev` 的关系

`pnpm dev` 仍然面向 Web 联合调试，入口是 `scripts/dev-runtime.js --profile web`：

1. 清理云端后端、本机后端和 Web 前端相关端口。
2. 构建统一 dev Rust 目标：`agentdash-api`、`agentdash-local`、`agentdash-local-tauri`。
3. 启动云端后端 `agentdash-server`。
4. 通过 `/api/local-runtime/ensure` 领取开发机的本机 runtime，拿到 server 颁发的 `backend_id`、relay token 和 WebSocket URL。
5. 启动 Web 前端 `app-web`。

`scripts/dev-runtime.js` 不再直接写 `/api/backends`，也不接受手工指定 `backend_id`。Web profile 会先调用 `agentdash-local machine-identity` 读取本机 runtime 自己识别和持久化的机器身份，再用该身份向 server ensure；`backend_id` 始终由 server 按 `machine_id + scope + capability_slot` 生成或复用。开发脚本不拥有机器身份文件路径，也不生成机器身份。

`pnpm dev:desktop` 面向 Tauri 桌面端调试，入口是 `scripts/dev-runtime.js --profile desktop`：

1. 生成 Web favicon 与 Tauri 桌面图标资源。
2. 清理独立后端、桌面 renderer 和残留 Tauri 壳。
3. 先统一编译 dev Rust 目标：`agentdash-api`、`agentdash-local`、`agentdash-local-tauri`，避免任一长驻进程编译期间就开始做健康探测。
4. 启动独立 `agentdash-server`，用于后端日志、断点和 API 调试。
5. 启动桌面 renderer `app-tauri`。
6. 启动 Tauri 壳 `agentdash-local-tauri`，并让它复用外部 `agentdash-server`。

开发期 `pnpm dev` 与 `pnpm dev:desktop` 都复用 `agentdash-local` crate 的机器身份逻辑。Tauri 壳不再维护第二套开发态 machine identity，避免同一台机器在 Web 联合调试和桌面调试中被 server 识别成两个 personal local runtime。

Tauri 壳启动后会初始化 embedded `DesktopRunnerHost`。`AGENTDASH_DESKTOP_API_MODE=external` 只表示 Dashboard API 指向外部 server，不会关闭桌面内嵌本机执行面。native host 的自动启动判断为：

- `desktop-app-settings.json` 中 `auto_connect_local_runtime=true` 是全局自动连接开关。
- `local-runtime-profile.json` 中 `auto_start=true` 表示这份 profile 参与 native 启动期自动连接。
- desktop embedded runner 的 enrollment origin 跟随当前 Desktop Dashboard API origin。开发期 `pnpm dev:desktop` 通过 `AGENTDASH_DESKTOP_API_ORIGIN` / `VITE_API_ORIGIN` 指向本机 `agentdash-server`，因此 runner ensure 也连接同一个本机 origin；release external 包的 Dashboard API origin 指向远端时，runner ensure 也连接同一个远端 origin。
- profile 里的 `workspace_roots`、`executor_enabled` 继续是启动配置事实源；`server_url` 会被规范化到当前 Dashboard API origin。profile 不保存 access token，用户登录后 Web bridge 只传当前 access token 并请求同一个 native 启动服务。

设置页的本机运行时诊断会展示 `idle`、`disabled`、`waiting_for_auth`、`waiting_for_api`、`claiming`、`starting`、`running`、`retrying`、`error`、`stopping`、`stopped` 等 native supervisor 状态，以及 `last_error`、retry count、next retry 和 relay 状态。诊断区只展示脱敏后的错误与日志；`desktop_access_token` 表示桌面登录授权路径，`runner_registration_token` 表示独立 runner 注册路径。

直接执行 `pnpm run dev:desktop-shell` 时不会自动设置 external 模式；这种方式会走 Tauri 壳的默认行为，由壳进程内托管 Dashboard API。需要调试独立后端时优先使用 `pnpm dev:desktop`。

Web 与 Desktop profile 都复用 `scripts/lib/dev-process.js` 中的开发进程基础设施：

- 子进程 supervisor。
- `Ctrl+C` 统一收尾。
- 子进程树停止。
- `runCommand`。
- HTTP ready 探测。
- JSON HTTP 请求。
- 按进程名清理残留进程树。

业务编排集中在 `scripts/dev-runtime.js`，通过 `web` / `desktop` profile 显式表达差异，避免两条启动路径继续漂移。

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
$env:AGENTDASH_DEFAULT_CLOUD_ORIGIN = "https://agentdash.example.com"
pnpm run desktop:build
```

生成 Windows NSIS 安装包：

```powershell
$env:AGENTDASH_DEFAULT_CLOUD_ORIGIN = "https://agentdash.example.com"
pnpm run desktop:bundle
```

`desktop:build` 与 `desktop:bundle` 都通过 `scripts/desktop-build.js` 进入 Tauri 构建，默认使用 `external` API mode，也就是打出的桌面壳连接配置好的远端 server。desktop embedded runner 的 ensure origin 跟随 Desktop Dashboard API origin；未单独指定 `--api-origin` 时，`AGENTDASH_DEFAULT_CLOUD_ORIGIN` 会作为这个 Dashboard API origin。

```powershell
pnpm run desktop:build -- --default-cloud-origin https://agentdash.example.com
pnpm run desktop:build -- --api-origin https://agentdash.example.com
pnpm run desktop:build -- --api-mode builtin
pnpm run desktop:build -- --api-mode sidecar --api-origin http://127.0.0.1:17301 --api-sidecar target/release/agentdash-server.exe
```

- `external`：默认形态，桌面壳连接远端 server。
- `builtin`：显式 opt-in 形态，桌面壳内置启动本机 API。
- `sidecar`：桌面壳启动指定 API 可执行文件，并在退出时一起终止。
- `--sccache` / `--no-sccache` / `--sccache-dir` 与开发启动脚本语义一致，用于控制 Rust 编译缓存。

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
