# Research: 发布产物与验收流程 planning review

- Query: 为子任务“发布产物与验收流程”补全 design/implement review，覆盖产物矩阵、版本一致性、安装器 vs app exe、runner release、Windows/Linux 服务验收、卸载清理、手工验收模板、release gate、实现小步与跨子任务 handoff。
- Scope: internal
- Date: 2026-06-26

## Findings

### Files Found

- `.trellis/tasks/06-26-distribution-release-validation/prd.md` - 当前 PRD 已定义三类产物、版本一致性、安装/服务/断网/卸载验收和非实现者可执行 checklist 目标。
- `.trellis/tasks/06-26-distribution-release-validation/design.md` - 当前 design 只有发布产物、验收矩阵、版本原则和 tradeoff 的骨架。
- `.trellis/tasks/06-26-distribution-release-validation/implement.md` - 当前 implement 只有高层 checklist，runner build 命令仍标记为后续固定。
- `.trellis/tasks/06-26-distribution-release-validation/implement.jsonl` - 已包含 project overview、tech stack、desktop local runtime 三个关键上下文。
- `.trellis/tasks/06-26-distribution-release-validation/check.jsonl` - 已包含 cross-layer thinking guide、backend/frontend quality guidelines。
- `package.json` - 根版本为 `0.1.0`，已有 `desktop:build` 和 `desktop:bundle` 脚本；`desktop:bundle` 当前固定 NSIS、无签名、CI 参数。
- `scripts/desktop-build.js` - 桌面构建入口固定 Tauri 配置路径，并默认使用 builtin Desktop API。
- `scripts/lib/desktop-build.js` - 桌面构建参数解析、Tauri build 调用、API mode/origin/sidecar 编译期默认值与 sccache 处理。
- `crates/agentdash-local-tauri/tauri.conf.json` - Tauri product/version/identifier/frontendDist/bundle target 配置。
- `Cargo.toml` - Rust workspace 版本为 `0.1.0`，workspace 成员包含 `agentdash-local` 和 `agentdash-local-tauri`。
- `crates/agentdash-local/Cargo.toml` - `agentdash-local` binary 使用 workspace version，默认 feature 包含 standalone。
- `crates/agentdash-local/src/main.rs` - runner CLI 目前只有 `machine-identity` 子命令和直连运行参数；运行时要求 `--cloud-url`、`--token`、`--backend-id`。
- `crates/agentdash-local/src/ws_client.rs` - runner WebSocket register payload 已上报 `env!("CARGO_PKG_VERSION")`。
- `crates/agentdash-local-tauri/src/main.rs` - 桌面端内置 Desktop API、runtime/profile/logs Tauri command、profile auto-start，以及 ensure/claim 请求中的 `client_version`。
- `scripts/dev-runtime.js` - 当前本机 runner 启动是开发链路：先 `/api/local-runtime/ensure` 领取 backend，再运行 target/debug 下的 `agentdash-local`，并检查 `/api/backends/online`。
- `scripts/lib/dev-process.js` - dev build 只编译 `agentdash-api`、`agentdash-local`、`agentdash-local-tauri` 的 debug 目标。
- `.trellis/spec/project-overview.md` - 项目是云端/本机双后端模型，本机后端是 per-machine 进程，预研阶段不需要兼容历史状态。
- `.trellis/spec/tech-stack.md` - 技术栈明确 Rust + Axum + Tokio + SQLx、React/Vite/Tauri，以及 crate 结构。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - 桌面本机 runtime 的关键 contract：Desktop API、Dashboard ready、机器身份、profile 路径、runtime_paths、Tauri CLI 依赖与验收矩阵。

### Code Patterns

- 根 npm 版本是 `0.1.0`，发布版本一致性可从根 `package.json` 开始校验。见 `package.json:3`。
- 桌面构建已有两个入口：`desktop:build` 使用 `--no-bundle --ci`，`desktop:bundle` 使用 `--bundles nsis --no-sign --ci`。见 `package.json:24`、`package.json:25`。
- Tauri CLI 是仓库依赖，不应要求全局 `cargo tauri`。见 `package.json:55` 与 `.trellis/spec/cross-layer/desktop-local-runtime.md:331`。
- 桌面构建脚本默认 Tauri config 路径为 `crates/agentdash-local-tauri/tauri.conf.json`，默认 API mode 是 `builtin`。见 `scripts/desktop-build.js:13`、`scripts/desktop-build.js:14`。
- 桌面构建通过环境变量把 `AGENTDASH_DESKTOP_DEFAULT_API_MODE` / `AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN` 注入 Rust build，Tauri build 通过 `pnpm exec tauri build --config ...` 执行。见 `scripts/lib/desktop-build.js:32`、`scripts/lib/desktop-build.js:33`、`scripts/lib/desktop-build.js:41`、`scripts/lib/desktop-build.js:43`、`scripts/lib/desktop-build.js:46`。
- `--api-mode sidecar` 必须提供 `--api-sidecar`，所以 design 应区分第一版发布用 builtin 还是 sidecar，不要让验收脚本隐式依赖不完整参数。见 `scripts/lib/desktop-build.js:153`。
- Tauri product 是 `AgentDash`，Tauri 配置版本也是 `0.1.0`，bundle target 当前是 `all`，但 npm 脚本只请求 NSIS。见 `crates/agentdash-local-tauri/tauri.conf.json:3`、`crates/agentdash-local-tauri/tauri.conf.json:4`、`crates/agentdash-local-tauri/tauri.conf.json:32`。
- Tauri build 前会构建 `app-tauri` 前端，产物来自 `packages/app-tauri/dist`。见 `crates/agentdash-local-tauri/tauri.conf.json:7`、`crates/agentdash-local-tauri/tauri.conf.json:9`。
- Rust workspace version 是 `0.1.0`，`agentdash-local` 和 `agentdash-local-tauri` 都使用 workspace version。见 `Cargo.toml:37`、`crates/agentdash-local/Cargo.toml:4`、`crates/agentdash-local-tauri/Cargo.toml:4`。
- `agentdash-local` 当前 standalone binary 定义存在，默认 feature 包含 standalone。见 `crates/agentdash-local/Cargo.toml:8`、`crates/agentdash-local/Cargo.toml:10`、`crates/agentdash-local/Cargo.toml:13`。
- runner CLI 当前只定义 `MachineIdentity` 子命令，未发现 service install/uninstall 子命令。见 `crates/agentdash-local/src/main.rs:39`、`crates/agentdash-local/src/main.rs:42`。
- runner 直接运行要求 `--cloud-url`、`--token`、`--backend-id`，其中 `backend_id` 必须来自 server ensure/claim。见 `crates/agentdash-local/src/main.rs:75`、`crates/agentdash-local/src/main.rs:76`、`crates/agentdash-local/src/main.rs:77`、`crates/agentdash-local/src/main.rs:90`。
- runner 注册 WebSocket 时上报 Rust crate version。见 `crates/agentdash-local/src/ws_client.rs:131`、`crates/agentdash-local/src/ws_client.rs:133`。
- 桌面端 ensure/claim 请求带 `client_version: Some(env!("CARGO_PKG_VERSION").to_string())`。见 `crates/agentdash-local-tauri/src/main.rs:331`、`crates/agentdash-local-tauri/src/main.rs:444`。
- 桌面端有 profile load/save、runtime start/stop/restart、logs_tail、desktop_api_snapshot commands，可作为 Windows Desktop 手工验收诊断入口。见 `crates/agentdash-local-tauri/src/main.rs:142`、`crates/agentdash-local-tauri/src/main.rs:154`、`crates/agentdash-local-tauri/src/main.rs:175`、`crates/agentdash-local-tauri/src/main.rs:185`、`crates/agentdash-local-tauri/src/main.rs:194`、`crates/agentdash-local-tauri/src/main.rs:287`、`crates/agentdash-local-tauri/src/main.rs:305`。
- 桌面端支持 profile auto-start，但不是 OS 登录自启动；OS 自启动需要另一个子任务明确实现或交付验收方式。见 `crates/agentdash-local-tauri/src/main.rs:115`、`crates/agentdash-local-tauri/src/main.rs:357`、`crates/agentdash-local-tauri/src/main.rs:359`。
- Desktop API 默认端口是 `127.0.0.1:3001`，spec 要求 DashboardHost 确认 `/api/health` ready 后再渲染 Web Dashboard。见 `.trellis/spec/cross-layer/desktop-local-runtime.md:20`、`.trellis/spec/cross-layer/desktop-local-runtime.md:22`。
- dev runner 流程先 POST `/api/local-runtime/ensure`，再把 `relay_ws_url`、`auth_token`、`backend_id` 传给 `agentdash-local`，最后查 `/api/backends/online`。见 `scripts/dev-runtime.js:135`、`scripts/dev-runtime.js:137`、`scripts/dev-runtime.js:138`、`scripts/dev-runtime.js:140`、`scripts/dev-runtime.js:155`、`scripts/dev-runtime.js:734`。
- dev runner 读取的是 `target/debug/agentdash-local(.exe)`，不是 release 产物。见 `scripts/dev-runtime.js:821`。
- dev Rust build 只编译 debug target 的 `agentdash-api`、`agentdash-local`、`agentdash-local-tauri`。见 `scripts/lib/dev-process.js:213`、`scripts/lib/dev-process.js:214`。

### 当前计划缺口 / 风险（按优先级）

1. **缺少 release artifact contract，容易把“能 build app exe”误当成“能交付安装器”。** 当前 design 只列出三类产物，没有写每个产物的文件名、平台、架构、入口命令、配置输入、诊断入口、版本展示方式、安装/卸载边界。Windows Desktop 必须明确验收对象是 NSIS installer，而不是 `target/release/agentdash-local-tauri.exe` 或 Tauri app exe。
2. **runner release/service 能力还没有被计划拆成可交付 contract。** 代码里当前 runner CLI 只有 `machine-identity` 和直连运行参数；未发现 `service install/start/status/stop/uninstall` 或 `--version`。Linux systemd 与 Windows Service 验收需要依赖其他子任务提供 release binary、service unit/installer/script 和命令语义。
3. **版本一致性需要变成 release gate，而不是原则句。** 根 `package.json`、Cargo workspace、Tauri config 当前都是 `0.1.0`，runner/desktop runtime 上报 CARGO_PKG_VERSION；但计划还缺少“release manifest”或“version check”步骤来证明桌面壳、内置前端、Desktop API、Local Runtime/Runner、协议生成物来自同一构建版本。
4. **安装器 vs app exe 的边界需要写清。** `desktop:bundle` 当前是 `--bundles nsis --no-sign --ci`，而 Tauri config `targets=all`。design 应明确发布验收以 NSIS 安装器为准，app exe 只作为调试/定位产物，不作为用户交付产物。
5. **OS 登录自启动与 profile auto-start 容易混淆。** 代码已有 profile `auto_start`，表示应用内 runtime 自动连接；PRD 提到“自启动”需要明确是 Windows 登录后 app 自启动，还是应用启动后 runtime auto-start。若是 OS 登录自启动，必须由相关子任务交付 installer/startup integration。
6. **卸载清理边界不足。** 当前 implement 只说清理自启动项、服务项和安装期创建文件，但需要写清不得删除用户工作区、不得删除 runtime 业务数据，还是仅删除服务注册、安装目录、安装期 scheduled task/startup item。由于 spec 把 `local-runtime` data root 定为本机生命周期事实源，卸载策略要区分 app uninstall 与 user data purge。
7. **手工验收模板还缺少可执行字段。** 需要为每一步提供：环境前置、命令/UI 操作、预期结果、证据记录、诊断入口、失败归因、是否 gate blocking。只写 checklist 名称不足以让非实现者执行。
8. **release gate 缺少“阻断条件”。** 需要定义哪些失败必须阻断发布：版本不一致、installer 无法安装/卸载、runner 不能 online、服务卸载残留、断网重连失败、诊断入口不可用、文档无法按步骤复现。
9. **dev 脚本不应进入用户发布流程。** spec 和 PRD 均要求发布流程不依赖开发脚本；当前 dev runner 流程可作为发现依据，但 release design 需要另设正式 release build/install 命令。
10. **验收环境矩阵缺失。** 至少需要 Windows Desktop、Windows Runner Service、Linux Runner systemd 三套独立环境；每套记录 OS 版本、架构、是否管理员/root、云端 server origin、网络断开/恢复方法。

### 推荐的更完整 Design 结构

可直接把 `design.md` 扩写为以下结构：

```markdown
# 发布产物与验收流程 - Design

## Scope

- 本子任务只收口发布产物定义、release gate 和手工验收流程。
- 不在本子任务内重新设计 Desktop runtime、runner service manager 或协议字段。
- 依赖其他子任务交付可构建/可安装/可托管的产物能力；本任务负责把这些能力串成 release contract。

## Release Artifact Matrix

| 产物 | 平台 | 交付文件 | 用户入口 | 构建命令 | 版本来源 | 诊断入口 | 验收状态 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Windows Desktop Installer | Windows x64 | NSIS installer，例如 `AgentDash_<version>_x64-setup.exe` | 安装器 + Start Menu/Desktop app | `pnpm run desktop:bundle` | root package/Cargo workspace/Tauri config 同一版本 | Desktop API `/api/health`、桌面 runtime logs、安装器日志 | required |
| Linux Local Runner | Linux x64 | `agentdash-local` release binary + service unit/install docs | systemd service | 由 runner release 子任务 handoff 固定 | Cargo workspace version + runtime register version | `journalctl -u <service>`、cloud backend runtime health、runner stdout/stderr | required |
| Windows Local Runner | Windows x64 | `agentdash-local.exe` release binary + service install docs/script | Windows Service | 由 runner release 子任务 handoff 固定 | Cargo workspace version + runtime register version | Windows Event Log/service stdout-stderr/cloud backend runtime health | required |

## Version Consistency Contract

- 单次 release 只有一个 `release_version`，必须同时写入/校验：
  - `package.json.version`
  - `Cargo.toml [workspace.package].version`
  - `crates/agentdash-local-tauri/tauri.conf.json.version`
  - runner binary `agentdash-local` 上报/输出版本
  - desktop ensure/claim `client_version`
  - protocol/generated contracts 对应同一源码构建
- release gate 必须产出 version evidence：
  - 构建前版本校验结果
  - Desktop installer metadata 或 installed app version
  - `agentdash-local --version` 或等价 runner version 命令输出
  - cloud runtime health 中 runner/desktop client version

## Desktop Installer vs App Exe

- 用户交付对象是 Windows Desktop Installer；直接 app exe 只作为构建中间产物和故障定位入口。
- 验收必须从干净环境运行 installer 开始，覆盖安装、启动、Desktop API ready、Dashboard 渲染、runtime profile save/start、profile auto-start、退出、卸载。
- installer 验收需要记录安装路径、开始菜单/桌面入口、自启动项、卸载入口和卸载后残留项。

## Runner Release Contract

- runner release 产物必须不依赖 repo checkout、`pnpm dev`、`target/debug` 或开发脚本。
- runner 运行配置必须来自用户可编辑配置文件、环境变量或 service install 参数，并包含：
  - cloud/server URL
  - token 或 token 文件路径
  - backend_id 或 ensure/claim 输入
  - workspace roots
  - executor enabled flag
  - log path
- runner 必须提供版本查看方式；推荐 `agentdash-local --version`。

## Linux Service Validation

- 前置：Linux x64、systemd、可访问云端、拥有安装权限。
- 安装：复制 runner binary 和配置，安装 systemd unit，reload daemon。
- 启动：`systemctl start <service>`；状态为 active/running。
- Online：云端 backend runtime health 显示 online，version 匹配 release_version。
- 断网重连：断开网络后云端转 offline 或 last_seen 停止更新；恢复网络后自动 reconnect 并回到 online。
- 停止/卸载：`systemctl stop`、disable、remove unit、daemon-reload；服务不再存在且 runner 进程退出。

## Windows Service Validation

- 前置：Windows x64、管理员权限、可访问云端。
- 安装：使用 handoff 的 service installer/script 注册 Windows Service。
- 启动：`Start-Service <service>`；状态为 Running。
- Online：云端 backend runtime health 显示 online，version 匹配 release_version。
- 断网重连：禁用网络或阻断 server origin 后再恢复，服务自动 reconnect。
- 停止/卸载：`Stop-Service`、uninstall/delete service；服务项消失且 runner 进程退出。

## Uninstall / Cleanup Boundary

- Desktop uninstall 必须清理安装目录、开始菜单/桌面入口、installer 注册项、安装期自启动项。
- Runner service uninstall 必须清理 service/unit 注册、service 管理脚本产生的运行时 pid/临时文件。
- 默认不删除用户 workspace、用户任务产物、server 业务数据。
- `local-runtime` data root 的处理策略必须明确：默认保留 profile/log/cache，若提供 purge 步骤则作为显式可选验收项。

## Manual Acceptance Template

| Step ID | Product | Environment | Action | Expected Result | Evidence | Diagnostics | Gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| WD-01 | Windows Desktop | Windows x64 clean VM | Run installer | Install succeeds and app entry exists | screenshot/log/path | installer log/Event Viewer | block |
| WD-02 | Windows Desktop | installed app | Launch app | Desktop API health ready; Dashboard renders | `/api/health`, screenshot | desktop logs | block |
| LR-01 | Linux Runner | systemd host | Install/start service | service active and cloud online | `systemctl status`, cloud backend row | `journalctl` | block |
| WR-01 | Windows Runner | admin PowerShell | Install/start service | service Running and cloud online | `Get-Service`, cloud backend row | Event Log/log path | block |

## Release Gate

- Build gate: release build commands pass and produce expected files.
- Version gate: all version evidence equals `release_version`.
- Install gate: each artifact installs from clean environment without source checkout.
- Runtime gate: Desktop and both runners can connect to cloud and show online.
- Resilience gate: network interruption recovers without manual process restart.
- Cleanup gate: uninstall/service uninstall removes registered app/service/startup artifacts and leaves no running process.
- Documentation gate: a non-implementer can execute the checklist and record evidence without asking for hidden steps.
```

### 推荐的 Implement 小步提交清单

1. **补齐 release planning docs**
   - 扩写 `design.md`：加入 artifact matrix、version consistency contract、installer/app exe boundary、runner release contract、service validation、cleanup boundary、manual acceptance template、release gate。
   - 扩写 `implement.md`：把高层 checklist 拆成可执行小步和 handoff gate。

2. **固化版本一致性检查**
   - 增加/确认 release version check：校验 `package.json`、Cargo workspace、Tauri config 一致。
   - 把 runner/desktop runtime 上报版本、protocol contracts 生成/检查结果纳入 release evidence。

3. **固化 Windows Desktop release build 与产物记录**
   - 明确 `pnpm run desktop:bundle` 的 expected output glob、产物命名、架构、NSIS installer metadata。
   - 记录安装器验收入口与诊断入口。

4. **接入 runner release build handoff**
   - 等 runner 子任务交付后，把 Linux/Windows release binary build 命令、输出路径、checksum/manifest 写入 implement。
   - 确认 release binary 不依赖 `target/debug`、repo checkout、dev-runtime。

5. **接入 Linux systemd handoff**
   - 写入 systemd unit/install/uninstall/status/log 命令。
   - 写入 Linux 验收步骤和失败诊断路径。

6. **接入 Windows Service handoff**
   - 写入 service install/uninstall/status/log 命令或脚本。
   - 写入 Windows 验收步骤和失败诊断路径。

7. **补齐卸载清理验收**
   - Desktop：安装目录、开始菜单/桌面入口、自启动项、卸载注册项、残留进程。
   - Runner：service/unit、运行进程、service 管理脚本产生的文件。
   - 明确 user data/workspace/runtime data root 的保留或显式 purge 策略。

8. **形成手工验收模板**
   - 为 Windows Desktop、Linux Runner、Windows Runner 各写一套 step table。
   - 每步必须有 Action、Expected Result、Evidence、Diagnostics、Gate。

9. **执行 release gate dry-run**
   - 在可用平台上跑构建和版本检查。
   - 对尚无平台条件的步骤标记为 blocked by environment，不把未验收项写成通过。

10. **收口 context manifests**
   - `implement.jsonl` 保留 spec + 本研究文件；必要时加入 runner/service 子任务研究文件。
   - `check.jsonl` 加入本研究文件和 release gate 文档，便于 check agent 对照计划审阅。

### 需要从其他子任务接收的 Handoff Contract

- **Windows Desktop packaging 子任务**
  - `pnpm run desktop:bundle` 的最终支持平台/架构和 output glob。
  - NSIS installer 文件命名规则、安装目录、开始菜单/桌面快捷方式策略。
  - 是否实现 Windows 登录自启动；若实现，注册位置和关闭/卸载行为。
  - installer 日志位置和失败诊断方式。
  - 卸载时清理项与保留项。

- **Runner release binary 子任务**
  - Linux/Windows release build 命令、输出路径、二进制名称、架构。
  - 版本输出命令；推荐 `agentdash-local --version`。
  - release manifest/checksum 格式。
  - runner 配置输入 contract：server URL、token/token file、backend_id/claim 流程、workspace roots、executor flag、log path。
  - 不依赖 dev-runtime、target/debug、repo checkout 的证明方式。

- **Linux service 子任务**
  - systemd unit 名称、安装路径、配置文件路径、日志入口。
  - install/start/status/stop/restart/uninstall 命令。
  - 断网重连预期时间窗口和失败诊断。
  - service uninstall 后哪些文件删除、哪些 user data 保留。

- **Windows service 子任务**
  - Windows Service 名称、显示名、安装路径、配置文件路径、日志入口。
  - install/start/status/stop/restart/uninstall 命令或 PowerShell/script。
  - 管理员权限要求和失败诊断。
  - 断网重连预期时间窗口。
  - service uninstall 后注册表/service 项/进程/文件清理策略。

- **Backend runtime health / cloud UI 子任务**
  - 用于判定 online/offline 的 API 或 UI 路径。
  - runtime health 是否展示 version、backend_id、last_seen、disconnect_reason。
  - 验收时如何稳定定位刚安装的 runner/desktop backend。

- **Protocol/contracts 子任务**
  - release 前必须执行的 contracts generate/check 命令。
  - protocol/generated 文件和 release_version 的 evidence 关系。
  - 如果 protocol 无运行时版本字段，需要明确以源码构建 manifest 或 generated hash 作为一致性证据。

### Suggested Paste-Ready Gaps Summary

- 当前 `design.md` 需要从“产物列表”升级为“release contract”：每类产物必须写明交付文件、平台/架构、构建命令、安装入口、运行配置、版本证据、诊断入口和 gate 状态。
- Windows Desktop 验收对象应明确为 NSIS installer；Tauri app exe 只能作为构建中间产物/诊断入口，不能代表用户交付形态。
- runner release/service 能力当前不能假设存在；计划必须把 Linux systemd 与 Windows Service 的 install/status/uninstall/log 命令列为跨子任务 handoff。
- 版本一致性要成为 release gate：根 npm version、Cargo workspace version、Tauri config version、runner/desktop runtime 上报 version、protocol contracts evidence 必须一致。
- “自启动”需拆成两层：应用内 profile auto-start 已有代码事实；Windows 登录自启动必须由 packaging 子任务交付明确注册/卸载 contract。
- 卸载清理要定义边界：清理安装器/服务/自启动注册和安装期文件；默认不删除用户工作区与任务产物；`local-runtime` data root 是否 purge 必须显式可选。
- 手工验收 checklist 每步必须包括命令或 UI 操作、预期结果、证据、诊断入口、是否阻断发布，才能满足非实现者执行要求。

## Related Specs

- `.trellis/spec/project-overview.md:43` - 云端/本机双后端模型。
- `.trellis/spec/project-overview.md:68` - 本机后端是 per-machine 进程，主动连接云端 WebSocket。
- `.trellis/spec/project-overview.md:105` - 预研阶段不需要兼容历史状态，保持正确状态。
- `.trellis/spec/tech-stack.md:72` - `agentdash-local` 是本机后端 crate。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:7` - Tauri 薄壳持有 LocalRuntimeManager 并启动 API。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:20` - Desktop API 默认 `127.0.0.1:3001`。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:22` - DashboardHost 必须等待 `/api/health` ready。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:28` - Tauri/dev scripts 只能通过 local library 或 `agentdash-local machine-identity` 获取机器身份。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:38` - `agentdash-local::runtime_paths` 是本机 runtime 路径事实源。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:321` - Desktop local runtime validation matrix。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:331` - Tauri CLI 缺失时仓库依赖 `@tauri-apps/cli`，不要求全局安装。

## External References

- No external docs were required for this planning review. Findings are based on repository task artifacts, build scripts, Rust/Tauri config, and Trellis specs.

## Caveats / Not Found

- No code or product documentation outside this research file was modified.
- `task.py current --source` returned no active task, so this research used the user-provided task path `.trellis/tasks/06-26-distribution-release-validation`.
- No runner release build script was found in the inspected files beyond development debug build paths.
- No `agentdash-local --version` CLI path was found; current clap derive may provide default help, but explicit version output should be verified/implemented by the runner release subtask.
- No Linux systemd service install/uninstall contract was found.
- No Windows Service install/uninstall contract was found.
- No Windows OS login autostart packaging contract was found; existing profile `auto_start` is application-level runtime auto-start after desktop app startup.
- No release manifest/checksum generation path was found.
