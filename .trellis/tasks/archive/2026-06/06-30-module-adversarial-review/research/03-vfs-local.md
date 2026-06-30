# Research: VFS & Runtime Tool Surface + Local Runtime & Relay Surface

- Query: 对抗性架构审查 VFS mount / providers / runtime tool composer / context file discovery / mount ownership，以及 agentdash-local / relay protocol / command handlers / terminal / materialization / runner claim / desktop shell；对照旧任务 06-14 baseline 标记 resolved / residual / resurfaced / superseded。
- Scope: internal
- Date: 2026-06-30

## Findings

### Boundary Judgment

VFS & Runtime Tool Surface 与 Local Runtime & Relay Surface 不应合并为单一模块。第一性边界是：

- VFS surface 拥有 model-visible address、mount set、provider dispatch、capability validation、VFS URI resolution 和物化计划。
- Local relay surface 拥有本机进程、物理路径、WebSocket relay command execution、terminal/session process table、runner credentials 和桌面壳生命周期。
- 二者的交界只应是 relay command / materialization payload / `mount_root_ref`。VFS 不应知道本机 profile/runner；local runtime 不应重新解释 VFS owner/purpose。

建议拆分审查后续实现任务为三个收束域：

1. VFS mount ownership metadata contract。
2. Local relay command scheduling / domain handler execution contract。
3. Desktop runner enrollment/profile ownership。

### Files Found

- `crates/agentdash-application/src/runtime_tools/provider.rs` - session runtime tool composer 与共享 session tool services handle。
- `crates/agentdash-application/src/runtime_tools/vfs_provider.rs` - VFS-only runtime tool provider，注入 VFS service/materialization/shell output registry。
- `crates/agentdash-api/src/bootstrap/session.rs` - runtime tool composer 装配 VFS / workflow / collaboration / task / workspace module providers。
- `crates/agentdash-application-vfs/src/tools/factory.rs` - VFS mounts/fs/shell tool factory。
- `crates/agentdash-application-vfs/src/mount.rs` - provider constants、mount owner/purpose 推断、project VFS mount 标记。
- `crates/agentdash-application-vfs/src/mount_project.rs` - project/story/workspace/agent knowledge runtime VFS 构建与 grants 应用。
- `crates/agentdash-application-vfs/src/mount_inline.rs` - inline/context mount owner metadata 写入与解析。
- `crates/agentdash-application-vfs/src/path.rs` - mount id/path/root_ref/capability/link hard validation。
- `crates/agentdash-application-vfs/src/service.rs` - VFS provider dispatch 与 read/write/list/search/patch entry。
- `crates/agentdash-application/src/context/mount_file_discovery.rs` - AGENTS/SKILL/memory VFS 文件自动发现与扫描策略。
- `crates/agentdash-api/src/bootstrap/vfs.rs` - VFS kernel bootstrap、provider registry、materialization relay transport 装配。
- `crates/agentdash-local/src/handlers/mod.rs` - local relay envelope router 与 domain command handlers 装配。
- `crates/agentdash-local/src/handlers/prompt.rs` - prompt/cancel/steer/discover relay command handler。
- `crates/agentdash-local/src/handlers/tool_calls.rs` - file/shell/search relay tool handler。
- `crates/agentdash-local/src/handlers/terminal.rs` - interactive terminal relay command handler。
- `crates/agentdash-local/src/handlers/materialization.rs` - VFS materialization relay command handler。
- `crates/agentdash-local/src/handlers/mcp_relay.rs` - MCP probe/list/call/close relay command handler。
- `crates/agentdash-local/src/handlers/extension.rs` - extension action/channel relay command handler 与 artifact activation。
- `crates/agentdash-local/src/ws_client.rs` - backend WebSocket loop、register payload、command scheduling。
- `crates/agentdash-local/src/tool_executor.rs` - local workspace-root-bounded file/search/process execution boundary。
- `crates/agentdash-local/src/shell_session_manager.rs` - shell tool 与 terminal 共用 session/process/output manager。
- `crates/agentdash-local/src/materialization.rs` - local materialization store, safe path/digest/cache manifest。
- `crates/agentdash-local/src/runner_claim.rs` - headless runner registration-token claim client。
- `crates/agentdash-local/src/runner_config.rs` - runner config merge, credentials, LocalRuntimeConfig projection。
- `crates/agentdash-local/src/desktop_runner_host.rs` - desktop embedded local runtime host wrapper。
- `crates/agentdash-local/src/runtime.rs` - LocalRuntimeConfig/manager/standalone runtime entry。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri commands, profile persistence, desktop API mode, desktop ensure/claim, tray/lifecycle。
- `crates/agentdash-relay/src/protocol.rs` - relay top-level wire envelope。
- `crates/agentdash-relay/src/protocol/extension_runtime.rs` - extension action/channel relay payload。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs` - cloud-side relay_fs provider dispatch to backend registry。

### Code Patterns

- Resolved: runtime tool composition no longer lives in VFS provider. `SessionRuntimeToolComposer` only iterates providers at `crates/agentdash-application/src/runtime_tools/provider.rs:43` and `crates/agentdash-application/src/runtime_tools/provider.rs:59`; VFS-specific provider is separate at `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:15` and `crates/agentdash-application/src/runtime_tools/vfs_provider.rs:61`.
- Resolved: session bootstrap now composes separate providers at `crates/agentdash-api/src/bootstrap/session.rs:434`, `:437`, `:443`, `:447`, `:448`, and wraps them with `SessionRuntimeToolComposer::new` at `:459`.
- Residual: mount owner/purpose still derives from provider string and raw metadata at `crates/agentdash-application-vfs/src/mount.rs:46`, `:63`, `:92`, and project-VFS status from metadata at `:121`.
- Residual: inline owner coordinates are written/read as JSON metadata at `crates/agentdash-application-vfs/src/mount_inline.rs:72` and `:94`; mutation dispatch re-parses them at `crates/agentdash-application-vfs/src/mutation_dispatcher.rs:27`.
- Resolved: VFS hard validation now covers duplicate mount id, default mount, provider root scheme and built-in capability range at `crates/agentdash-application-vfs/src/path.rs:369`, `:419`, and `:451`.
- Residual: context file discovery has a central provider allowlist and metadata override keys at `crates/agentdash-application/src/context/mount_file_discovery.rs:21`, `:22`, `:312`, and `:339`.
- Resolved: local command handling is now a `LocalCommandRouter` with domain handlers at `crates/agentdash-local/src/handlers/mod.rs:49`, configured at `:75`, and dispatched by envelope match at `:118`.
- Residual: relay command scheduling is still centrally decided in WebSocket loop; only shell commands enter background handling at `crates/agentdash-local/src/ws_client.rs:373` and `:498`.
- Resolved: shell tool and terminal share `ShellSessionManager` at `crates/agentdash-local/src/shell_session_manager.rs:25`, `:248`, and `:297`; terminal handler consumes the same manager at `crates/agentdash-local/src/handlers/terminal.rs:20`.
- Resolved: materialization uses VFS-side planning plus local safe materialization. VFS chooses direct backend path vs materialization at `crates/agentdash-application-vfs/src/materialization.rs:65`, `:209`, `:444`; local store verifies path/digest and ignores backend_id in constructor at `crates/agentdash-local/src/materialization.rs:50`, `:67`, `:215`, `:287`, `:381`.
- Residual: desktop Tauri still defines and persists profile and claim DTOs in shell code at `crates/agentdash-local-tauri/src/main.rs:109`, `:126`, `:244`, `:256`, `:425`, `:447`, `:638`, `:662`, and `:752`; headless runner claim separately exists at `crates/agentdash-local/src/runner_claim.rs:51` and `:128`.
- Resolved: extension schema baseline has moved forward: gateway validates action/channel input at `crates/agentdash-application-runtime-gateway/src/runtime_gateway/extension_actions.rs:169` and `:649`; local host validates action/channel output at `crates/agentdash-local/src/extensions/host/manager.rs:124`, `:136`, `:155`, and `:168`.

### Related Specs

- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/permission/architecture.md`

### External References

- None. 本轮为仓库内部架构审查。

## Issues

### 1. P1 - WebSocket read loop still owns cross-domain command scheduling

- 问题分类: 横向耦合 / 模块过厚 / 职责漂移。
- Baseline 对照: residual / resurfaced。06-14 的 “local CommandHandler 是全域 command hub” 在结构上已拆成 `LocalCommandRouter + domain handlers`，但耗时命令调度仍回到 `ws_client` 的中央 enum 策略。
- 代码证据:
  - WebSocket read loop 在收到 message 后用 `should_handle_in_background(&relay_msg)` 决定是否 `tokio::spawn`，否则 inline await handler：`crates/agentdash-local/src/ws_client.rs:373`。
  - `should_handle_in_background` 只覆盖 shell exec/read/input/terminate：`crates/agentdash-local/src/ws_client.rs:498` 和 `crates/agentdash-local/src/ws_client.rs:501`。
  - Prompt handler 会做 workspace prepare、session launch、forwarder claim，且都从 inline handler 入口开始：`crates/agentdash-local/src/handlers/prompt.rs:77`、`:123`、`:187`。
  - MCP probe 有 15s timeout，当前不在 background allowlist：`crates/agentdash-local/src/handlers/mcp_relay.rs:13`、`:27`。
  - Extension action handler 可能下载 artifact、activate host、invoke action，当前也不在 background allowlist：`crates/agentdash-local/src/handlers/extension.rs:50`、`:78`、`:189`。
  - VFS materialization command 可能写本地 cache/workdir，router 分发点在 `crates/agentdash-local/src/handlers/mod.rs:200`，local store 执行入口在 `crates/agentdash-local/src/materialization.rs:67`。
- 为什么是第一性问题:
  - Relay router 的不可省职责是 envelope dispatch；命令的耗时、并发、取消、session ordering 属于各 domain handler 的执行 contract。
  - 当前中央 allowlist 把 “哪些命令会阻塞 read loop” 变成 `ws_client` 对所有 command 的知识，新增 domain command 时很容易忘记调度语义。
- 影响面:
  - 一个 MCP probe、extension artifact activation、materialization 或 prompt prepare 可能延迟后续 cancel、terminal input、ping response 或 session event flush。
  - 读循环里的调度策略与 domain handler 的实际耗时行为分离，测试也只能覆盖 enum allowlist，无法覆盖 handler 自己的执行语义。
- 建议收束边界:
  - `LocalCommandRouter` 返回 `CommandDispatchPlan` 或 domain handler 自带 `ExecutionMode`，由 handler 声明 `inline_control` / `background` / `per_session_ordered`。
  - WebSocket loop 只执行统一 dispatch plan，不维护 command enum allowlist。
  - 对 prompt/session、terminal、extension、MCP/materialization 分别定义 ordering key：例如 session_id、terminal_id、extension invocation id；取消类命令保持优先路径。

### 2. P1 - Desktop shell still owns profile persistence and desktop ensure/claim protocol

- 问题分类: 重复事实源 / 模块过厚 / 职责漂移。
- Baseline 对照: residual。06-14 “Tauri main 重新实现 profile/claim 协议，desktop shell 不够薄” 当前仍成立，只是 `DesktopRunnerHost` 已吸收 runtime host 生命周期。
- 代码证据:
  - Tauri shell 自定义 `RuntimeStartRequest`、`LocalRuntimeProfile`：`crates/agentdash-local-tauri/src/main.rs:109`、`:126`。
  - Tauri shell 直接实现 `profile_load` / `profile_save` / `profile_delete` 并读写 profile 文件：`crates/agentdash-local-tauri/src/main.rs:244`、`:256`、`:268`。
  - Tauri shell 自定义 ensure/claim request/response DTO：`crates/agentdash-local-tauri/src/main.rs:425`、`:447`。
  - Tauri shell 自己执行 `start_runtime_from_request`、`claim_local_runtime`、`post_local_runtime_claim`、`validate_claim_response`：`crates/agentdash-local-tauri/src/main.rs:638`、`:662`、`:752`、`:716`。
  - Headless runner claim 在 `agentdash-local` 内另有一套 request body、HTTP status mapping 和 credentials projection：`crates/agentdash-local/src/runner_claim.rs:14`、`:51`、`:128`、`:170`。
  - `DesktopRunnerHost` 已存在并负责 embedded runtime host wrapper：`crates/agentdash-local/src/desktop_runner_host.rs:15`、`:31`。
- 为什么是第一性问题:
  - 机器身份、server-issued backend_id、relay_ws_url、auth_token、registration_source、claimed_at 是 local runtime 事实，不是 Tauri shell 事实。
  - Desktop 与 runner 的 auth source 不同，但 claim 后的 relay credential / LocalRuntimeConfig projection 应由同一个 local runtime library owner 维护。
- 影响面:
  - Desktop ensure/claim 与 runner claim 对字段校验、registration_source、scope/capability_slot、错误分类和 token redaction 可能继续分叉。
  - Tauri `main.rs` 同时承载 tray/settings/autostart/API host/profile/MCP/log/runtime/open-url，后续桌面壳变更会误碰 runner enrollment 语义。
  - pre-release 阶段没有兼容包袱，继续保留双实现会固化错误 owner。
- 建议收束边界:
  - 在 `agentdash-local` 下建立 `desktop_profile` / `desktop_claim` 或统一 `enrollment` 模块，导出 desktop access-token ensure 与 runner registration-token claim 的 shared credential projection。
  - Tauri commands 只保留 DTO adapter、invoke boundary、tray/API shell lifecycle；profile path、normalize、claim response validation、LocalRuntimeConfig construction 下沉到 `agentdash-local`。
  - Headless runner 与 desktop embedded runner 共享 redaction/error/status vocabulary，只在 auth adapter 上分叉。

### 3. P2 - Mount ownership and purpose are still inferred from provider strings plus raw metadata

- 问题分类: 重复事实源 / 抽象泄漏 / 命名或职责漂移。
- Baseline 对照: residual。06-14 的 “`vfs/mount.rs` 聚合 provider mount 构建、metadata 编码和 UI 语义推断” 已通过文件拆分和 hard validation 明显改善，但 owner/purpose 事实源仍是 raw metadata + provider string。
- 代码证据:
  - `mount_owner_kind` 先按 provider string 判定 workspace/session/canvas/project，再回读 `agentdash_context_owner_kind` metadata：`crates/agentdash-application-vfs/src/mount.rs:46`、`:63`。
  - `mount_purpose` 再从 owner_kind、provider string 和 `is_project_vfs_mount` 推断用途：`crates/agentdash-application-vfs/src/mount.rs:92`、`:107`。
  - `is_project_vfs_mount` 依赖 `agentdash_project_vfs_mount` metadata bool：`crates/agentdash-application-vfs/src/mount.rs:121`。
  - Project mount builder 写入 project-vfs marker 与 owner metadata：`crates/agentdash-application-vfs/src/mount_project.rs:193`、`:203`、`:218`、`:232`。
  - Inline mount owner 解析在 `mount_inline`，mutation dispatcher 又重新解析 owner 坐标：`crates/agentdash-application-vfs/src/mount_inline.rs:72`、`:94`；`crates/agentdash-application-vfs/src/mutation_dispatcher.rs:27`。
  - VFS validation 只校验 root_ref scheme / capability 与部分 provider 约束，不校验 owner metadata 与 provider 的组合合法性：`crates/agentdash-application-vfs/src/path.rs:369`、`:419`、`:451`。
- 为什么是第一性问题:
  - Runtime mount 是 VFS dispatch、UI/resource surface、inline persistence、agent capability exposure 的共同对象；owner/purpose 不是展示装饰，而是业务边界。
  - 如果 owner/purpose 通过任意 metadata key 推断，provider 注册、integration mount 和 project mount builder 都可能在没有 typed contract 的情况下塑造上层语义。
- 影响面:
  - 新 provider 或新 mount purpose 需要同步修改 owner/purpose helper、surface summary、mutation dispatcher、context discovery policy 和 tests。
  - Surface/DTO 消费方会把 provider string 当业务 owner，弱化 `RuntimeMount` 作为分发单位的契约。
  - Project VFS mount、agent knowledge、story override、skill asset、canvas mount 的 owner 坐标仍有局部私有解析路径。
- 建议收束边界:
  - 引入 typed `RuntimeMountMetadata` / `MountOwnership` / `MountPurpose`，provider builder 内构造 typed facts，只在 SPI/DTO 边界序列化。
  - `validate_vfs` 校验 built-in provider 与 typed metadata 的合法组合，包括 project-vfs marker、inline owner、skill asset project id、canvas/lifecycle/routine scope。
  - `mount_owner_kind` / `mount_purpose` 退化为 typed metadata 的 projection，不再根据 provider string 和 ad-hoc metadata 推断。

### 4. P2 - Context file discovery policy is owned by a central provider allowlist instead of mount/provider contract

- 问题分类: 重复事实源 / 抽象泄漏 / 横向耦合。
- Baseline 对照: residual by extension。06-14 指向 mount metadata raw parsing 和 frontend/default mount selection 分散；当前新增的 context discovery 把另一个 mount usage policy 放进中心 allowlist，属于同类事实源分散。
- 代码证据:
  - Discovery policy metadata keys 定义在 context discovery 文件内：`crates/agentdash-application/src/context/mount_file_discovery.rs:21`、`:22`。
  - `discover_mount_files`、`discover_skill_vfs_files`、`discover_memory_vfs_files` 都调用同一个 `should_scan_mount_for_discovery`：`crates/agentdash-application/src/context/mount_file_discovery.rs:103`、`:169`、`:194`、`:206`。
  - `should_scan_mount_for_discovery` 先读 magic metadata，再按 provider 常量 allowlist 默认扫描 relay/inline/lifecycle/canvas/skill_asset：`crates/agentdash-application/src/context/mount_file_discovery.rs:312`、`:327`、`:339`。
  - Memory discovery 支持 recursive scan、max_depth/max_files，但是否允许扫描仍不由 provider 声明：`crates/agentdash-application/src/context/mount_file_discovery.rs:246`、`:258`、`:264`、`:265`。
- 为什么是第一性问题:
  - “某 mount 是否可自动扫描” 是 mount/provider 的 runtime capability 与成本/安全契约，不是 context discovery helper 的私有 allowlist。
  - AGENTS/SKILL/memory discovery 是 frame/context construction 的输入；扫描策略错误会直接影响 model-visible context，而不只是资源浏览 UI。
- 影响面:
  - 新 provider 若想默认参与/退出 auto discovery，需要写 magic metadata 或修改中心文件，provider 自身不能声明稳定策略。
  - 高成本或敏感 provider 可能因复用 built-in provider string 或缺少 metadata 而被递归扫描；低成本 custom provider 默认不可扫，能力被隐藏。
  - Skill、guideline、memory discovery 三路共享一个粗粒度 policy，难以表达“允许 AGENTS.md 但不允许 recursive memory scan”。
- 建议收束边界:
  - 将 discovery policy 放到 typed mount metadata 或 `MountProvider` descriptor，例如 `auto_discovery: None | Guidelines | Skills | Memory | All` 与 cost/risk hints。
  - `discover_*` 只消费 provider/mount 给出的 typed policy；metadata override 也应先解析进 typed metadata。
  - 为 recursive memory scan 单独增加 provider-declared max depth/file cap 上限，调用方规则只能在 provider 上限内收窄。

### 5. P2 - Local relay capability registration mixes static local inventory with runtime-resolved command surface

- 问题分类: 概念分叉 / 重复事实源。
- Baseline 对照: superseded / residual。06-14 的 MCP resolved transport 问题已由 spec 与代码改成 command payload 携带 resolved server；但 backend register payload 仍用 local static inventory 表达 capability discovery。
- 代码证据:
  - Local backend register payload 使用 `build_capabilities(&handler, &config.mcp_manager)`：`crates/agentdash-local/src/ws_client.rs:297`、`:301`。
  - `build_capabilities` 从 `handler.list_executors()` 和 `mcp_manager.capability_entries()` 生成 capabilities：`crates/agentdash-local/src/ws_client.rs:508`、`:512`、`:515`。
  - `McpClientManager::capability_entries()` 只来自本机静态 config：`crates/agentdash-local/src/mcp_client_manager.rs:47`。
  - MCP command execution 已消费 payload 中的 resolved `server`，而不是只靠 local static catalog：`crates/agentdash-local/src/handlers/mcp_relay.rs:128`、`:158`。
- 为什么是第一性问题:
  - Register capabilities 用于 backend selection / availability display，runtime command payload 用于具体执行。二者可以不同，但必须命名为不同事实：static local inventory vs runtime-resolved execution surface。
  - 现在同一个 `CapabilitiesPayload.mcp_servers` 容易被理解成执行事实，实际执行事实来自 session AgentFrame/resolved MCP payload。
- 影响面:
  - Backend selection 可能因 static inventory 缺少 project-scoped resolved server 而低估本机可执行性，或因 static inventory 存在而高估当前 session 可用 resolved server。
  - Debug 时很难判断某 MCP failure 是 register capability、project runtime MCP surface、protect-mode allowlist 还是 resolved transport 本身的问题。
- 建议收束边界:
  - 将 register payload 中 MCP 字段重命名/投影为 `local_static_mcp_inventory` 或 `setup_capabilities`，不要叫作 runtime MCP capability。
  - Backend allocator 对 MCP 的使用应明确是 “筛选有本机静态 server 名” 还是 “允许任意 project resolved transport”；protect mode 才消费 static allowlist。
  - `/backends/runtime-summary` 继续合并 static inventory、runtime health、leases，但字段要区分 inventory 与 runtime-resolved availability。

## Baseline Regression Matrix

- `RelayRuntimeToolProvider` 膨胀为跨域 tool composer: resolved。当前为 `SessionRuntimeToolComposer + Vfs/Workflow/Collaboration/Task/WorkspaceModule providers`。
- local `CommandHandler` 全域状态和路由过厚: structurally resolved, scheduling residual/resurfaced。状态拆到 domain handlers，但 WebSocket loop 仍以中央 enum allowlist 决定并发。
- Extension workspace/process/env Host API 过宽: mostly resolved/superseded for本轮范围。Host API 不再接受 raw workspace root 覆盖的证据未作为本轮主问题；process/env 权限需由 Extension Runtime 专题继续审查。
- Extension input/output schema 未执行: resolved。Gateway 校验 input，local host 校验 output。
- `vfs/mount.rs` 过厚与 raw metadata: partially resolved, residual。文件拆分和 validation 已改善，owner/purpose 仍 raw metadata。
- Tauri `main.rs` profile/claim 重复实现: residual。
- 前端 VFS browser / extension webview mount selection 分散: not reviewed in this sub-scope; likely superseded by Project/Workspace/Frontend surface review。
- VFS materialization cloud plan / local store 双段式: resolved/not problem。当前边界清晰，local store ignores backend_id by design and validates path/digest。
- MCP resolved transport: resolved for command execution; register capability naming remains residual conceptual ambiguity。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task; this file was written to the explicit user-provided task path.
- Did not run full tests, per instruction.
- This review intentionally did not edit business code, specs, config, or git state.
- Some command output from very large files was truncated by the terminal; all cited evidence lines were rechecked with `rg -n`.
- Frontend VFS browser / extension webview mount selection was not deeply reviewed because the requested scope emphasized VFS/local/relay business code.
