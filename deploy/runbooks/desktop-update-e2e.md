# 桌面自动更新端到端验证 Runbook

本文用于验证 Windows stable 桌面端自动更新闭环。它覆盖真实 Tauri updater 签名包、云端 update endpoint、强制更新阻断、安装后显式重启和负向错误路径。

本 Runbook 使用本地或企业测试环境承载 stable `latest.json` 与 updater artifact。主仓不记录企业对象存储 endpoint、bucket、AK/SK 或私有域名；迁移到企业仓库时由私有发布流程注入这些事实。

## 关联入口

- 发布总流程：[release-workflow.md](./release-workflow.md)
- 桌面运行时规格：`.trellis/spec/cross-layer/desktop-local-runtime.md`
- 桌面自动更新任务：`.trellis/tasks/07-06-desktop-auto-update-release/prd.md`

## 验证目标

- 旧版本桌面端只通过 AgentDash 云端 `/api/desktop/update` 与 `/api/desktop/update/tauri` 获取更新信息。
- 旧版本低于显式 `AGENTDASH_MIN_DESKTOP_VERSION` 时进入强制更新阻断态，不启动本机 runtime、不 claim、不建立 Relay。
- 签名正确的 updater artifact 可以被 `tauri-plugin-updater` 下载、校验、安装。
- 安装完成后桌面端等待用户点击重启，不静默重启。
- 篡改 updater artifact、signature、manifest 或下载 URL 时，桌面端拒绝安装并展示可诊断错误。

## 实现边界

- 安装路径由 `desktop_update_install` Tauri command 进入 Rust `tauri-plugin-updater`，原因是桌面端需要按当前云端 API origin 动态构造 `/api/desktop/update/tauri` endpoint。
- 前端只通过 Tauri `invoke()` 调用更新命令并维护 UI 状态；`packages/app-tauri` 不依赖 JS `@tauri-apps/plugin-updater` 包。
- 安装完成后的显式重启由 `@tauri-apps/plugin-process` 的 `relaunch()` 承担；`@tauri-apps/plugin-process` 依赖属于桌面端重启入口。

## 前置条件

- Windows 验证机器可安装并启动旧版 AgentDash 桌面端。
- release 环境能构建两个版本的桌面端，例如 `0.1.0` 作为旧版、`0.1.1` 作为新版。
- release 环境配置 Tauri updater signing key，能产出 updater zip 与 `.sig`。
- 旧版桌面端运行环境包含 updater public key：

```text
AGENTDASH_DESKTOP_UPDATER_PUBKEY=<base64 minisign public key>
```

- 测试用 AgentDash 云端服务可以配置：

```text
AGENTDASH_DESKTOP_STABLE_MANIFEST_URL=<stable latest manifest HTTP URL>
AGENTDASH_DESKTOP_MANIFEST_CACHE_TTL_SECONDS=1
AGENTDASH_MIN_DESKTOP_VERSION=<new version>
AGENTDASH_RECOMMENDED_DESKTOP_VERSION=<new version>
```

## 主仓预检查

进入真实签名包和企业对象存储验证前，先在主仓确认自动更新相关逻辑与文档契约：

```bash
node --test scripts/lib/release-metadata.test.js
pnpm run release:metadata -- --desktop-release-dir dist/release/desktop
pnpm run contracts:check
cargo test -p agentdash-local-tauri desktop_update
cargo check -p agentdash-local-tauri
pnpm --filter app-tauri typecheck
pnpm --filter @agentdash/views typecheck
pnpm run desktop:check
```

## 构建与发布准备

1. 构建旧版桌面端并安装到验证机器。

   旧版需要能连接测试云端 API，并内置或运行期提供 `AGENTDASH_DESKTOP_UPDATER_PUBKEY`。

2. 构建新版桌面端。

   验证以下产物存在：

   ```text
   target/release/bundle/nsis/<installer>.exe
   target/release/bundle/nsis/<updater>.nsis.zip
   target/release/bundle/nsis/<updater>.nsis.zip.sig
   ```

3. 生成桌面发布目录。

   ```bash
   pnpm run release:metadata -- --desktop-release-dir dist/release/desktop
   ```

4. 检查发布目录。

   ```text
   dist/release/desktop/releases/agentdash/<version>/release.json
   dist/release/desktop/releases/agentdash/<version>/release.json.sha256
   dist/release/desktop/releases/agentdash/<version>/windows/x86_64/<installer>.exe
   dist/release/desktop/releases/agentdash/<version>/windows/x86_64/<installer>.exe.sha256
   dist/release/desktop/releases/agentdash/<version>/windows/x86_64/<updater>.nsis.zip
   dist/release/desktop/releases/agentdash/<version>/windows/x86_64/<updater>.nsis.zip.sha256
   dist/release/desktop/releases/agentdash/<version>/windows/x86_64/<updater>.nsis.zip.sig
   dist/release/desktop/channels/stable/latest.json
   dist/release/desktop/upload-plan.json
   ```

5. 发布到测试 HTTP 访问面。

   本地验证可以用静态 HTTP server 承载 `dist/release/desktop`。企业仓库验证应按 `upload-plan.json` 先上传 versioned artifacts，再覆盖 stable latest 指针。

6. 配置云端服务端读取 stable latest manifest。

   ```text
   AGENTDASH_DESKTOP_STABLE_MANIFEST_URL=<HTTP base>/channels/stable/latest.json
   AGENTDASH_MIN_DESKTOP_VERSION=<new version>
   AGENTDASH_RECOMMENDED_DESKTOP_VERSION=<new version>
   ```

## 正向验证流程

1. 检查云端产品 update endpoint。

   ```text
   GET /api/desktop/update?platform=windows&arch=x86_64&current_version=<old version>
   ```

   期望：

   - `status` 为 `update_available` 或等价可更新状态。
   - `latest.version` 为新版。
   - `policy.min_desktop_version` 为新版。
   - `policy.min_desktop_version_configured=true`。
   - `diagnostics.manifest_url_configured=true`。

2. 检查 Tauri updater endpoint。

   ```text
   GET /api/desktop/update/tauri?platform=windows&arch=x86_64&current_version=<old version>
   ```

   期望响应包含：

   ```text
   version
   notes
   pub_date
   platforms.windows-x86_64.url
   platforms.windows-x86_64.signature
   ```

3. 启动旧版桌面端。

   期望：

   - Dashboard API health ready 后刷新 update policy。
   - 显示强制更新屏。
   - 不渲染普通 Web Dashboard。
   - 本机 runtime 不自动启动。

4. 在强制更新屏点击安装更新。

   期望：

   - 进入安装中状态。
   - updater artifact 下载并通过签名校验。
   - 安装成功后显示“重启完成更新”。
   - 应用不自动重启。

5. 点击重启完成更新。

   期望：

   - 桌面端显式 relaunch。
   - 新版启动后 current version 为新版。
   - 不再触发强制更新阻断。
   - runtime start/restart 可用。

## 强制更新阻断验证

在旧版客户端、云端显式配置 `AGENTDASH_MIN_DESKTOP_VERSION=<new version>` 时验证：

- `runtime_start` 返回强制更新错误，不发起 claim。
- `runtime_restart` 返回强制更新错误。
- settings/profile/MCP/log-clear 等突变命令返回强制更新错误。
- `desktop_update_policy_refresh`、`desktop_update_install`、`desktop_quit_request`、`runtime_snapshot`、`logs_tail`、`desktop_api_snapshot`、`desktop_update_policy_snapshot` 保持可用。
- Web bridge 自动连接不会调用 native runtime start。

## 非阻断更新验证

配置 `AGENTDASH_RECOMMENDED_DESKTOP_VERSION=<new version>`，但不配置 `AGENTDASH_MIN_DESKTOP_VERSION`。

期望：

- 设置页或桌面更新区域显示可更新状态。
- 普通 Dashboard 可用。
- runtime start/restart 可用。
- 更新检查失败只在更新区域或诊断中显示，不打断主界面。

## 负向验证矩阵

| 条件 | 期望 |
| --- | --- |
| `AGENTDASH_DESKTOP_UPDATER_PUBKEY` 缺失 | 安装命令在下载前失败，提示未配置签名公钥 |
| stable manifest URL 未配置 | 产品 endpoint 返回 `unconfigured` 诊断，Tauri endpoint 返回 204，本地调试不阻断 |
| stable manifest HTTP 拉取失败 | 产品 endpoint 返回 `fetch_failed` 诊断，非强制场景不打断主界面 |
| stable manifest JSON 或 schema 无效 | 产品 endpoint 返回 `invalid_manifest`，Tauri endpoint 返回 204 |
| manifest 缺少 `windows-x86_64` artifact | 产品 endpoint 返回 `unsupported_target`，Tauri endpoint 返回 204 |
| 当前版本等于或高于 latest | 产品 endpoint `update_available=false`，Tauri endpoint 返回 204 |
| updater artifact URL 返回 404 | 安装失败并展示下载阶段错误 |
| updater artifact 被篡改 | Tauri updater 拒绝安装并展示下载或签名校验错误 |
| `.sig` 与 artifact 不匹配 | Tauri updater 拒绝安装并展示签名校验错误 |
| sha256 与 artifact 不一致 | 发布前 upload/manifest 校验失败；客户端侧仍以 Tauri signature 为安装可信门槛 |
| 安装成功但用户未点重启 | UI 停留在 `ready_to_restart`，应用不自动重启 |
| relaunch 失败 | UI 保留错误或停留在可重试状态，不宣称更新完成 |

## 企业仓库迁移提示

企业仓库应把本 Runbook 中的静态 HTTP server 替换为企业 S3-compatible 上传流程：

```text
1. 消费 dist/release/desktop/upload-plan.json
2. 上传 immutable versioned artifacts
3. 校验远端 artifact 可读与 sha256
4. 覆盖 channels/stable/latest.json
5. 校验 stable latest manifest HTTP URL
6. 配置云端 AGENTDASH_DESKTOP_STABLE_MANIFEST_URL
7. 执行正向与负向验证矩阵
```

企业仓库可以保留真实 endpoint、bucket、AK/SK、ACL、CDN 或内网访问方式；这些事实不要回写主仓。
