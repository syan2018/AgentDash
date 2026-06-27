# Research: VFS / Local Runtime / Relay / Extension Host 过度设计审查

- Query: 审查 VFS / Local Runtime / Relay / Extension Host 中的过度设计、过厚模块、重复事实源、跨层耦合与职责漂移，重点关注 VFS mount/tool composer、local runtime command handler、relay raw JSON 边界、extension permission/process/env contract。
- Scope: internal
- Date: 2026-06-14

## Findings

### 摘要判断

本链路整体已经形成了正确的大方向：VFS surface 以 `surface_ref + mount_id + mount_relative_path` 为外部访问模型，local 文件工具以 `mount_root_ref` 限制执行边界，relay 协议的工具类 payload 多数已经使用 `deny_unknown_fields`，extension host 也按 action/channel method 做运行时权限裁决。

主要问题不在“缺少抽象”，而在若干抽象已经吸收了过多职责：`RelayRuntimeToolProvider`、local `CommandHandler`、`vfs/mount.rs` 和 Tauri `main.rs` 都承担了跨域装配职责；Extension runtime 的 schema / permission / workspace / process contract 仍存在“声明了但没有形成窄执行语义”的问题。预研期不需要兼容层，建议后续清理直接把事实源收回到单一 typed contract 或按 cluster/domain 拆分。

### Files Found

- `crates/agentdash-application/src/vfs/tools/factory.rs` - VFS read/write/execute 工具的窄工厂。
- `crates/agentdash-application/src/vfs/tools/provider.rs` - 当前 session runtime tool provider，实际混合 VFS、workflow、collaboration、workspace module、extension runtime。
- `crates/agentdash-application/src/vfs/tools/common.rs` - tool path resolution 与共享 runtime VFS handle。
- `crates/agentdash-application/src/vfs/mount.rs` - workspace/project/story/lifecycle/routine/skill/canvas mount 构建与 mount metadata 解析。
- `crates/agentdash-application/src/vfs/service.rs` - VFS provider dispatch、overlay、read/write/list/search/patch/exec 统一服务。
- `crates/agentdash-application/src/vfs/surface.rs` - Resolved VFS surface DTO 与 `surface_ref` 解析。
- `crates/agentdash-application/src/vfs/surface_query.rs` - mount summary 投影，包含 backend online、file count、edit capability。
- `crates/agentdash-application/src/vfs/mutation_dispatcher.rs` - surface mutation 与 inline storage key 派生入口。
- `crates/agentdash-api/src/routes/vfs_surfaces.rs` - VFS surface HTTP route handler。
- `crates/agentdash-api/src/vfs_surface_runtime.rs` - API 层 VFS runtime projection，提供 backend online 与 provider edit capability。
- `crates/agentdash-local/src/handlers/mod.rs` - local relay command dispatcher 与共享 command state。
- `crates/agentdash-local/src/handlers/tool_calls.rs` - relay file/shell/search tool command handler。
- `crates/agentdash-local/src/handlers/extension.rs` - extension action/channel relay command handler 与 artifact activation。
- `crates/agentdash-local/src/tool_executor.rs` - local workspace-root-bounded file/shell/search executor。
- `crates/agentdash-local/src/process_executor.rs` - local process exec/shell implementation。
- `crates/agentdash-local/src/extensions/host/*` - TS extension host manager/process/protocol/host API/permission/process/workspace runtime。
- `crates/agentdash-local/src/runtime.rs` - local runtime lifecycle manager/config/status。
- `crates/agentdash-local/src/runtime_paths.rs` - local runtime data/config/profile path source。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri command、profile、claim、desktop API sidecar 管理。
- `crates/agentdash-relay/src/protocol.rs` - relay 顶层 wire envelope。
- `crates/agentdash-relay/src/protocol/tool.rs` - relay file/shell/search tool payload。
- `crates/agentdash-relay/src/protocol/prompt.rs` - relay prompt payload。
- `crates/agentdash-relay/src/protocol/extension_runtime.rs` - extension action/channel relay payload。
- `packages/extension-sdk/src/index.ts` - extension authoring SDK types。
- `packages/extension-dev/src/manifest.js` - extension manifest/package validation。
- `packages/app-web/src/features/vfs/*` - VFS browser UI 与 mount selection policy。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` - extension webview VFS/backend target selection。
- `packages/app-web/src/generated/vfs-contracts.ts` - generated VFS frontend contracts。

### Related Specs

- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`

### External References

- 未使用外部资料。本轮为仓库内部架构审查。

## Issues

### 1. P1 - `RelayRuntimeToolProvider` 已从 VFS/relay 工具提供者漂移成跨域 session tool composer

- 问题类型: 模块过厚 / 职责漂移 / 跨层耦合。
- 证据路径:
  - `crates/agentdash-application/src/vfs/tools/provider.rs`
  - `crates/agentdash-application/src/vfs/tools/factory.rs`
- 具体代码证据:
  - `RelayRuntimeToolProvider` 持有 `VfsService`、`RepositorySet`、`SessionToolServices`、`InlineContentPersister`、`PlatformConfig`、`FunctionRunner`、`ShellOutputRegistry`、`VfsMaterializationService`、`RuntimeGateway` handle、`ExtensionRuntimeChannelTransport` 等跨域依赖：`crates/agentdash-application/src/vfs/tools/provider.rs:55`。
  - `build_tools()` 先通过 `VfsToolFactory` 组装 VFS read/write/execute 工具：`crates/agentdash-application/src/vfs/tools/provider.rs:183`。
  - 同一个方法继续组装 workflow lifecycle tool：`crates/agentdash-application/src/vfs/tools/provider.rs:200`，companion collaboration tools：`crates/agentdash-application/src/vfs/tools/provider.rs:217`，workspace module tools：`crates/agentdash-application/src/vfs/tools/provider.rs:247`。
  - workspace module invoke 在 VFS provider 内等待 `RuntimeGateway` 和 extension channel transport，并从 session VFS / backend execution 解析调用 backend：`crates/agentdash-application/src/vfs/tools/provider.rs:304`。
  - 相比之下，`VfsToolFactory` 本身只处理 `mounts.list`、`fs.read/glob/grep/apply_patch`、`shell.exec`，职责较窄：`crates/agentdash-application/src/vfs/tools/factory.rs:46`。
- 影响面:
  - VFS 工具、Workflow、Companion、Workspace Module、Extension runtime 的装配生命周期被绑定在同一个 provider 中。
  - 添加或调整任一 tool cluster 都需要理解 VFS provider 的全量依赖注入顺序和 session context，`RuntimeGateway` 还需要延迟注入，增加初始化环风险。
  - 名称 `RelayRuntimeToolProvider` 已不能表达实际职责，未来读者会误判 VFS/relay 是这里的主要边界。
- 建议清理方向:
  - 保留 `VfsToolFactory` 作为 VFS cluster 的唯一工具工厂。
  - 新增薄的 `SessionRuntimeToolComposer` 或 `CompositeRuntimeToolProvider`，只负责按 cluster 调用多个专用 provider。
  - 拆出 `WorkflowToolProvider`、`CollaborationToolProvider`、`WorkspaceModuleToolProvider`，每个 provider 只持有所需依赖。
  - `RuntimeGateway` / extension channel transport 注入只进入 workspace-module provider，不再污染 VFS provider。

### 2. P1 - local `CommandHandler` 是全域 command hub，状态和路由都过厚

- 问题类型: 模块过厚 / 横向耦合。
- 证据路径:
  - `crates/agentdash-local/src/handlers/mod.rs`
  - `crates/agentdash-local/src/handlers/tool_calls.rs`
  - `crates/agentdash-local/src/handlers/extension.rs`
- 具体代码证据:
  - `CommandHandler` 同时持有 backend identity、workspace roots、`ToolExecutor`、session runtime、connector、MCP manager、workspace contract config、event channel、terminal manager、materialization store、session forwarders、extension host、extension artifact API/token/cache root：`crates/agentdash-local/src/handlers/mod.rs:40`。
  - `CommandHandlerConfig` 暴露同样的大配置面：`crates/agentdash-local/src/handlers/mod.rs:58`。
  - `handle()` 对 `RelayMessage` 做集中 match，覆盖 prompt/cancel/discover、workspace detect、file/shell/search、VFS materialize、MCP、extension、terminal 等命令：`crates/agentdash-local/src/handlers/mod.rs:114`。
  - tool handler 直接从共享 `CommandHandler` 取 `tool_executor` 和 `event_tx`：`crates/agentdash-local/src/handlers/tool_calls.rs:26`、`crates/agentdash-local/src/handlers/tool_calls.rs:195`。
  - extension handler 直接从共享 `CommandHandler` 取 artifact API、token、cache root、extension host、backend id、workspace roots：`crates/agentdash-local/src/handlers/extension.rs:158`、`crates/agentdash-local/src/handlers/extension.rs:216`。
- 影响面:
  - 新增 relay command 必然触碰中央 handler 和大状态结构，容易把某个域的配置泄漏给其它域。
  - command handler 单元测试与错误语义会被跨域状态拖累，难以判断某个 handler 的最小依赖。
  - extension artifact token、MCP manager、terminal manager、session forwarders 等生命周期边界混在一起，增加本机 runtime 停止/重启时的清理风险。
- 建议清理方向:
  - 保留 relay `RelayMessage` 顶层 enum，但将 local 执行侧拆为 `PromptCommandHandler`、`WorkspaceCommandHandler`、`ToolCommandHandler`、`McpCommandHandler`、`ExtensionCommandHandler`、`TerminalCommandHandler`、`MaterializationCommandHandler`。
  - 中央 `LocalCommandRouter` 只做 envelope 分派，不持有所有 domain state。
  - 用 `LocalCommandContext` 只承载 backend id、event tx、runtime shutdown 等真正共享事实，domain-specific 配置进入各自 handler。

### 3. P1 - Extension workspace/process/env Host API contract 过宽，session workspace 边界可被 raw params 改写

- 问题类型: 权限 contract 过宽 / 抽象泄漏 / 执行边界未定义。
- 证据路径:
  - `crates/agentdash-local/src/extensions/host/host_api.rs`
  - `crates/agentdash-local/src/extensions/host/workspace_api.rs`
  - `crates/agentdash-local/src/extensions/host/process_api.rs`
  - `crates/agentdash-local/src/tool_executor.rs`
  - `crates/agentdash-local/src/process_executor.rs`
  - `packages/extension-sdk/src/index.ts`
- 具体代码证据:
  - Host API 接受 `workspace_root` 参数覆盖默认 session workspace root：`crates/agentdash-local/src/extensions/host/host_api.rs:86`、`crates/agentdash-local/src/extensions/host/host_api.rs:93`。
  - Workspace API 的 read/write/list/stat 都通过该 `resolve_workspace_root()` 决定实际 root：`crates/agentdash-local/src/extensions/host/workspace_api.rs:15`、`crates/agentdash-local/src/extensions/host/workspace_api.rs:30`、`crates/agentdash-local/src/extensions/host/workspace_api.rs:46`、`crates/agentdash-local/src/extensions/host/workspace_api.rs:69`。
  - `ToolExecutor::validate_workspace_root()` 在 `workspace_roots` 未配置时接受任意存在的目录作为 workspace root：`crates/agentdash-local/src/tool_executor.rs:98`。
  - Process API 仅用 `process.execute` 同时保护 raw shell 和 argv exec：`crates/agentdash-local/src/extensions/host/process_api.rs:19`、`crates/agentdash-local/src/extensions/host/process_api.rs:59`。
  - Process env overlay 用 `env.read` / `env.read:{key}` 作为设置环境变量的权限要求：`crates/agentdash-local/src/extensions/host/process_api.rs:139`。
  - SDK 顶层 permission 声明有 `env` 的 read/write/read_write 和 `process` 的 execute，但 process options 只表达 `cwd/env/timeout/max_output`，没有命令、cwd scope、env write/set 的窄 contract：`packages/extension-sdk/src/index.ts:47`、`packages/extension-sdk/src/index.ts:216`。
  - 底层 process executor 的 `exec()` 可以执行任意 command，`shell_exec()` 可以执行任意 shell string，仅 cwd 被限制为 workspace root 内：`crates/agentdash-local/src/process_executor.rs:93`、`crates/agentdash-local/src/process_executor.rs:114`。
- 影响面:
  - Extension 默认应跟随 relay payload 的 session workspace root，但 raw host API params 允许插件尝试覆盖 root；在 `workspace_roots` 为空时，本机工具执行器会接受任意存在目录。
  - `env.read` 同时表示读取 host env 和注入 process env，语义不一致，审计时难以说明插件到底读取了什么还是只是设置了什么。
  - `process.execute` 是过粗权限，无法表达只允许某个 executable、禁止 shell、限制 cwd 子树、限制输出大小等执行面。
- 建议清理方向:
  - Extension Host API 不再接受任意 `workspace_root` 参数；workspace root 只能来自 activation 的 session workspace context。
  - 如果确实需要多 root，改为 typed `workspace_handle` / `workspace_id`，由 host 根据 session/runtime facts 解析，不接受路径字符串。
  - 拆分 permission：`process.exec`、`process.shell`、`process.cwd:<scope>`、`process.command:<name-or-pattern>`，并把 `env.get` 与 `process.env.set:<key>` 分开。
  - SDK、manifest validator、Rust domain permission classifier、host API guard 一次性收敛到同一组 permission key。

### 4. P1 - Extension input/output schema 是声明事实，但 relay/local runtime 没有执行校验

- 问题类型: 重复事实源 / raw JSON 边界过宽。
- 证据路径:
  - `packages/extension-sdk/src/index.ts`
  - `crates/agentdash-relay/src/protocol/extension_runtime.rs`
  - `crates/agentdash-application/src/runtime_gateway/extension_actions.rs`
  - `crates/agentdash-local/src/extensions/host/manager.rs`
- 具体代码证据:
  - SDK 要求 runtime action 声明 `input_schema` 与 `output_schema`，同时 `invoke(input)` 是泛型 JSON：`packages/extension-sdk/src/index.ts:30`。
  - protocol channel method 同样声明 `input_schema` 与 `output_schema`：`packages/extension-sdk/src/index.ts:56`。
  - Relay action payload 的 `input` 与 response `output` 都是 `serde_json::Value`：`crates/agentdash-relay/src/protocol/extension_runtime.rs:36`、`crates/agentdash-relay/src/protocol/extension_runtime.rs:55`。
  - Relay channel payload 的 `input` 与 response `output` 也是 `serde_json::Value`：`crates/agentdash-relay/src/protocol/extension_runtime.rs:66`、`crates/agentdash-relay/src/protocol/extension_runtime.rs:83`。
  - Runtime gateway 找到 action 后只验证 action kind、artifact 和 permissions，随后把 `request.input.clone()` 直接放入 transport request：`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:154`、`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:169`、`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:172`。
  - `validate_action_permissions()` 只检查 action permission 声明，没有 schema validation：`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:261`。
  - Local host manager 将 `input: Value` 直接传给 JS runner，`invoke_channel()` 也同理：`crates/agentdash-local/src/extensions/host/manager.rs:114`、`crates/agentdash-local/src/extensions/host/manager.rs:136`。
- 影响面:
  - manifest schema、SDK 泛型、relay wire payload、local JS runtime 之间没有形成单一可执行 contract。
  - 插件调用方可能以为 schema 会被平台执行，实际错误会落到插件内部或后续消费者，诊断点偏晚。
  - output schema 未执行时，前端 panel、workflow/canvas 调用方和审计 metadata 都只能消费 raw JSON。
- 建议清理方向:
  - 在 cloud RuntimeGateway 对 action/channel input 执行 JSON Schema validation，失败返回 capability/invalid request 错误。
  - 在 local runner response 回来后，对 output 执行 schema validation，再进入 relay response。
  - 如果暂时不执行 schema，则不要把 schema 当作 contract 名义暴露，应明确降级为 authoring/doc metadata；预研期更建议直接实现校验。

### 5. P2 - `vfs/mount.rs` 聚合了过多 provider mount 构建、metadata 编码和 UI 语义推断

- 问题类型: 模块过厚 / 重复事实源 / 抽象层泄漏。
- 证据路径:
  - `crates/agentdash-application/src/vfs/mount.rs`
  - `crates/agentdash-application/src/vfs/surface_query.rs`
  - `crates/agentdash-application/src/vfs/mutation_dispatcher.rs`
- 具体代码证据:
  - `build_derived_vfs()` 同时处理 workspace mount、project VFS mount、story disabled/override container、target-specific external mount filtering、default mount：`crates/agentdash-application/src/vfs/mount.rs:58`。
  - 同文件继续构建 Project VFS Mount：`crates/agentdash-application/src/vfs/mount.rs:283`，Context Container mount：`crates/agentdash-application/src/vfs/mount.rs:373`，Agent knowledge mount：`crates/agentdash-application/src/vfs/mount.rs:131`。
  - inline owner 坐标通过 `serde_json::Value` metadata 写入和解析：`crates/agentdash-application/src/vfs/mount.rs:428`、`crates/agentdash-application/src/vfs/mount.rs:450`。
  - UI/诊断语义 `mount_owner_kind()` / `mount_purpose()` 也在 mount builder 文件内按 provider string + metadata 推断：`crates/agentdash-application/src/vfs/mount.rs:490`、`crates/agentdash-application/src/vfs/mount.rs:536`。
  - lifecycle/routine/skill/canvas mount 构建继续落在同一文件：`crates/agentdash-application/src/vfs/mount.rs:826`、`crates/agentdash-application/src/vfs/mount.rs:881`、`crates/agentdash-application/src/vfs/mount.rs:986`、`crates/agentdash-application/src/vfs/mount.rs:1074`。
  - surface summary 又依赖 `inline_storage_key_from_mount()` 和 `mount_purpose()` 从 mount metadata 推出 file count 与 purpose：`crates/agentdash-application/src/vfs/surface_query.rs:32`、`crates/agentdash-application/src/vfs/surface_query.rs:61`。
  - mutation dispatcher 也从 mount metadata 还原 inline storage key：`crates/agentdash-application/src/vfs/mutation_dispatcher.rs:24`、`crates/agentdash-application/src/vfs/mutation_dispatcher.rs:110`。
- 影响面:
  - mount metadata 已成为 provider dispatch、inline persistence、surface summary、UI purpose 的共同事实源，但目前是 string key + raw JSON object。
  - 新增 provider/mount type 时很容易漏改 owner/purpose、inline key、surface summary、validation 测试。
  - `mount.rs` 文件规模和职责跨度过大，难以看出“VFS address 模型”和“各业务 provider mount builder”之间的边界。
- 建议清理方向:
  - 按 provider/owner 拆分 mount builder：`mount/workspace.rs`、`mount/project_vfs.rs`、`mount/context_container.rs`、`mount/lifecycle.rs`、`mount/routine.rs`、`mount/skill_asset.rs`、`mount/canvas.rs`。
  - 引入 typed metadata enum/struct，例如 `RuntimeMountMetadata::Inline { owner_kind, owner_id, container_id, purpose }`，只在 SPI/DTO 边界序列化成 JSON。
  - `mount_purpose` 和 inline storage key 从 typed metadata 派生，避免多个消费点各自解析 raw metadata。

### 6. P2 - Tauri `main.rs` 重新实现 profile/claim 协议，desktop shell 不够薄

- 问题类型: 模块过厚 / 重复事实源 / 职责漂移。
- 证据路径:
  - `crates/agentdash-local-tauri/src/main.rs`
  - `crates/agentdash-local/src/runtime.rs`
  - `crates/agentdash-local/src/runtime_paths.rs`
  - `packages/core/src/local-runtime/index.ts`
- 具体代码证据:
  - Tauri main 定义 `RuntimeStartRequest` 与 `LocalRuntimeProfile`，含 server/access_token/profile/machine/backend/relay/workspace/executor/auto_start 字段：`crates/agentdash-local-tauri/src/main.rs:74`。
  - 同文件直接实现 profile load/save/delete，并写 `local-runtime-profile.json`：`crates/agentdash-local-tauri/src/main.rs:140`、`crates/agentdash-local-tauri/src/main.rs:152`、`crates/agentdash-local-tauri/src/main.rs:164`。
  - 同文件定义 ensure/claim payload/response DTO：`crates/agentdash-local-tauri/src/main.rs:321`。
  - `start_runtime_from_request()`、`claim_local_runtime()`、`post_local_runtime_claim()` 和 `validate_claim_response()` 均在 Tauri main 内实现：`crates/agentdash-local-tauri/src/main.rs:408`、`crates/agentdash-local-tauri/src/main.rs:428`、`crates/agentdash-local-tauri/src/main.rs:475`、`crates/agentdash-local-tauri/src/main.rs:505`。
  - profile/start request normalization 在 Tauri main 内直接读取 machine identity：`crates/agentdash-local-tauri/src/main.rs:536`、`crates/agentdash-local-tauri/src/main.rs:559`。
  - `agentdash-local` 已拥有 local runtime config/lifecycle：`crates/agentdash-local/src/runtime.rs:24`，并拥有 local runtime profile path：`crates/agentdash-local/src/runtime_paths.rs:49`。
  - TS port 也定义同名 runtime start/profile DTO：`packages/core/src/local-runtime/index.ts:21`、`packages/core/src/local-runtime/index.ts:32`。
- 影响面:
  - Desktop spec 要求 Tauri 作为薄壳持有 `LocalRuntimeManager`；当前 profile 和 claim 协议的关键逻辑在 Tauri main 内，导致本机 runtime library 不是完整事实源。
  - CLI/dev script/Tauri 若都需要 ensure/claim 行为，容易出现字段、scope、capability_slot、token rotation、machine identity normalization 的重复实现。
  - `main.rs` 同时管理 desktop API sidecar、profile/MCP/log/runtime/open URL，文件已经超过薄 command shell 的合理职责。
- 建议清理方向:
  - 在 `agentdash-local` 新增 `profile` 与 `claim`/`server_registration` 模块，封装 profile load/save/normalize 和 ensure/claim request/response。
  - Tauri command 只做 `invoke` DTO 适配和调用 local library。
  - TS `LocalRuntimeClient` DTO 继续作为前端 port；Rust 侧 canonical profile/claim DTO 由 `agentdash-local` 导出。

### 7. P2 - 前端 VFS browser 与 extension webview 各自实现 mount 选择策略，surface 默认目标存在重复事实源

- 问题类型: 重复事实源 / 跨层耦合。
- 证据路径:
  - `packages/app-web/src/generated/vfs-contracts.ts`
  - `packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts`
  - `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts`
- 具体代码证据:
  - 后端给前端的 surface summary 只提供 `default_mount_id`、mount `provider/backend_online/purpose/edit_capabilities` 等基础事实：`packages/app-web/src/generated/vfs-contracts.ts:25`、`packages/app-web/src/generated/vfs-contracts.ts:27`、`packages/app-web/src/generated/vfs-contracts.ts:29`。
  - VFS browser policy 用 provider string 和 `backend_online` 判断 browsable，并有一套 provider 优先级：`packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:8`、`packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:12`、`packages/app-web/src/features/vfs/vfs-browser-panel-policy.ts:32`。
  - Extension webview bridge 的 VFS target 选择只按 `default_mount_id`、第一个有 backend_id 的 mount、首个 mount 回退，没有复用 browsable/offline policy：`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:223`。
  - Extension webview bridge 的 backend target 也按 default mount / backend_id 选择，并用 `backend_online !== false` 作为在线判断：`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:243`。
- 影响面:
  - 同一个 runtime surface 在 VFS Browser 与 Extension Panel 中可能选择不同 mount，尤其是 default mount 离线、存在 inline/lifecycle/canvas mount 时。
  - provider string 选择策略散落在 UI feature 和 extension runtime bridge，新增 provider 或改变默认浏览策略时需要多处同步。
  - 后端已经声明 `ResolvedVfsSurface` 是前端浏览/摘要/诊断共享的唯一 mount 真相源，但“哪个 mount 适合自动浏览 / extension 默认 workspace target”还没有成为 surface contract。
- 建议清理方向:
  - 后端 surface summary 增加 typed usage hints，例如 `browsable`, `auto_browse_rank`, `extension_workspace_target`, `runtime_backend_target`。
  - 或至少把前端 mount selection 策略收敛为一个共享 selector，extension bridge 和 VFS browser 共同调用。
  - 预研期优先把策略上移到后端 projection，因为 backend 更清楚 provider、backend online、session placement 与 workspace binding 事实。

## Code Patterns

- 窄 VFS tool factory 已存在：`VfsToolFactory::build_tools()` 只按 Read/Write/Execute cluster 装配 VFS 工具，见 `crates/agentdash-application/src/vfs/tools/factory.rs:46`。
- 外层 runtime tool provider 膨胀为跨域 composer：`RelayRuntimeToolProvider::build_tools()` 继续装配 workflow/collaboration/workspace module，见 `crates/agentdash-application/src/vfs/tools/provider.rs:158`。
- Relay 工具类 payload 已较严格：file/shell/search payload 使用 `#[serde(deny_unknown_fields)]`，见 `crates/agentdash-relay/src/protocol/tool.rs:5`。
- Relay extension runtime payload 仍以 raw JSON input/output 为主：`crates/agentdash-relay/src/protocol/extension_runtime.rs:42`、`crates/agentdash-relay/src/protocol/extension_runtime.rs:60`。
- Local command dispatcher 已按文件拆模块，但共享 state 和 route match 仍集中在 `CommandHandler`，见 `crates/agentdash-local/src/handlers/mod.rs:40` 与 `crates/agentdash-local/src/handlers/mod.rs:114`。
- Extension Host permission guard 按当前 action 或 channel method 声明裁决：`crates/agentdash-local/src/extensions/host/permission_guard.rs:9`。
- Extension Host process/env/workspace API 通过 generic `serde_json::Value` params 解析：`crates/agentdash-local/src/extensions/host/host_api.rs:14`。
- VFS route handler 基本没有直接操作 inline repo，而是通过 `VfsMutationDispatcher` 写入：`crates/agentdash-api/src/routes/vfs_surfaces.rs:343`。

## Not Problems / Boundaries Not Recommended To Change Now

- `VfsToolFactory` 本身不是过度设计。它是当前少数边界清晰的部分，问题在外层 `RelayRuntimeToolProvider` 聚合了非 VFS cluster。
- `MountProvider` composite registry 暂不建议拆掉。spec 已说明业务调用点可以依赖窄 trait 面，但分发路径需要同一个 provider 对象服务 discovery、IO 与搜索。
- Relay 顶层 `RelayMessage` enum 保持 centralized wire envelope 是合理边界。协议子 payload 已拆入 `protocol/*`，顶层集中有助于 wire format 稳定；问题不在 enum 存在，而在 local 执行侧 command handler 过厚。
- Extension 顶层 capability 不作为 deny path 暂不建议改成硬权限。`.trellis/spec/cross-layer/desktop-local-runtime.md` 已定义 trusted local extension 模型，运行时裁决使用 action/method 的 `permissions` 声明；真正需要收窄的是 process/env/workspace permission 的语义和 schema 执行。
- VFS surface route handler 与 mutation dispatcher 的边界总体健康。route 做权限、surface 解析和 DTO 转换，写入经 dispatcher；不建议把 inline owner 坐标重新上移到 route。
- VFS materialization 的 cloud plan / local store 双段式不是问题。spec 明确 materialization key 不使用 backend_id，`MaterializationStore::new(_backend_id)` 忽略 backend id 与契约一致。
- MCP relay resolved transport 现状不作为本轮问题。spec 明确 cloud 发送 resolved server declaration，local 按 transport hash 建连接池；代码已有对应结构。

## Follow-up Task Candidates

- 拆分 `RelayRuntimeToolProvider`：新增 per-cluster tool provider，并让 session runtime composer 只负责按 cluster 汇总。
- 拆分 local command handling：把 `CommandHandler` 改成 `LocalCommandRouter + domain handlers`，先从 extension/materialization/terminal 这类依赖最独立的命令开始。
- 收窄 Extension Host API contract：移除 raw `workspace_root` 覆盖，拆分 process/env permission key，并补齐 manifest validator、domain classifier、SDK types、host guard。
- 执行 extension JSON schema：RuntimeGateway 校验 input，local host 校验 output；同时为 channel method 做同等校验。
- 拆分 `vfs/mount.rs` 并引入 typed runtime mount metadata，消除 inline owner/purpose 的 raw JSON 解析扩散。
- 下沉 Tauri profile/claim 到 `agentdash-local`：Tauri main 只保留 command adapter 和 desktop API lifecycle。
- 统一 frontend surface mount selection：后端 surface summary 增加 usage hints，或前端共享一个 selector，覆盖 VFS browser 与 extension webview。

## Caveats / Not Found

- `task.py current --source` 返回 `Current task: (none)`，本报告按用户明确指定的 `.trellis/tasks/06-14-module-overdesign-review` 目录写入。
- 用户指定的 `.trellis/spec/backend/vfs/materialization.md` 在仓库中不存在；实际读取的是 `.trellis/spec/backend/vfs/vfs-materialization.md`，该路径也被 VFS architecture 文档引用。
- `crates/agentdash-api/src/vfs_access/mod.rs` 当前更像 VFS 集成测试集合，不是生产 route handler；生产 VFS surface route 位于 `crates/agentdash-api/src/routes/vfs_surfaces.rs`。
- 本轮未运行测试，符合只读架构 review 范围；结论基于源码和 spec 取证。
