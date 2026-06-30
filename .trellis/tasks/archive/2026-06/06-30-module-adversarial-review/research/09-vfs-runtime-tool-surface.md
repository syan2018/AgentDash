# Research: VFS & Runtime Tool Surface

- Query: 单域对抗性架构审查：VFS mount / VFS providers / runtime tool composer / context file discovery / mount ownership；检查 mount 权限、owner、capability、tool 暴露之间的重复路径，runtime tool composer 与 VFS provider 是否职责漂移，context file discovery 是否反向塑造 VFS/capability，并对照 06-14 baseline。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 06-14 baseline 总报告，含 VFS/runtime tool composer 历史问题排序。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` - 06-14 VFS / Local / Relay / Extension 深度 baseline。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - AgentRun / Session / Runtime Gateway baseline，用于 runtime tool/admission 边界对照。
- `.trellis/tasks/06-14-module-overdesign-review/research/04-frontend-contracts-permission.md` - Permission / capability baseline，用于授权事实源对照。
- `.trellis/spec/backend/vfs/architecture.md` - VFS mount/provider/tool composer 的当前目标边界。
- `.trellis/spec/backend/capability/architecture.md` - CapabilityState、AgentRun effective capability/admission 与 runtime tool surface 契约。
- `.trellis/spec/backend/permission/architecture.md` - PermissionGrant 与 runtime capability transition/admission 的授权事实源契约。
- `.trellis/spec/cross-layer/architecture.md` - 云端/本机 VFS 边界、HTTP DTO 和 local runtime 边界。
- `crates/agentdash-application/src/runtime_tools/provider.rs` - `SessionRuntimeToolComposer` 和共享 runtime tool helper。
- `crates/agentdash-application/src/runtime_tools/vfs_provider.rs` - VFS runtime tool provider，仅组装 VFS tool factory。
- `crates/agentdash-api/src/bootstrap/session.rs` - session bootstrap 中组合 VFS / workflow / collaboration / task / workspace-module tool providers。
- `crates/agentdash-application-vfs/src/tools/factory.rs` - VFS mounts/fs/shell 工具工厂和 capability gating。
- `crates/agentdash-application-vfs/src/tools/mounts.rs` - `mounts_list` 工具，向 Agent 暴露 mount capabilities。
- `crates/agentdash-application-vfs/src/mount_project.rs` - Project/Story/Task VFS 构建、project VFS mount、agent VFS grant 裁剪。
- `crates/agentdash-application-vfs/src/mount.rs` - mount owner/purpose 从 provider string 与 metadata 投影。
- `crates/agentdash-application-vfs/src/path.rs` - VFS hard validation，包括 mount/default/provider capability/link 检查。
- `crates/agentdash-application-vfs/src/service.rs` - VFS provider dispatch 与 mount capability enforcement。
- `crates/agentdash-application/src/context/mount_file_discovery.rs` - 通用 mount file discovery 规则与 discovery policy 判定。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs` - runtime skill/memory/guideline projection，消费 VFS discovery。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface_update.rs` - live VFS skill entry refresh。
- `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs` - frame construction 中 VFS 构建、agent VFS grant 裁剪、skill/memory projection。
- `crates/agentdash-application/src/frame_construction/request_assembler.rs` - companion child selected ProjectAgent 的 VFS grant 和 skill baseline projection。
- `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs` - runtime/MCP tool surface assembly 与 schema dedupe。
- `crates/agentdash-agent/src/agent.rs` / `crates/agentdash-agent/src/agent_loop/tool_call.rs` - callable tool list 保存与 tool call name lookup。

### Baseline Comparison

- Resolved: 06-14 的 P1 `RelayRuntimeToolProvider` 跨域 composer 问题已收束。当前 `SessionRuntimeToolComposer` 只聚合 providers（`crates/agentdash-application/src/runtime_tools/provider.rs:59`），`VfsRuntimeToolProvider` 只构造 `VfsToolFactory`（`crates/agentdash-application/src/runtime_tools/vfs_provider.rs:61`），bootstrap 明确注入 VFS/workflow/collaboration/task/workspace-module provider（`crates/agentdash-api/src/bootstrap/session.rs:434`、`:459`）。
- Resolved: 06-14 的 `vfs/mount.rs` 过厚问题已部分收束。当前 mount builder 已拆到 `mount_project.rs`、`mount_workspace.rs`、`mount_skill_asset.rs`、`mount_routine.rs` 等文件；`build_derived_vfs` 和 `build_workspace_vfs` 都调用 `validate_vfs`（`crates/agentdash-application-vfs/src/mount_project.rs:93`、`crates/agentdash-application-vfs/src/mount_workspace.rs:17`）。
- Residual: mount owner/purpose 仍由 provider string + raw metadata 推断，而不是 typed runtime mount metadata（`crates/agentdash-application-vfs/src/mount.rs:46`、`:92`）。这比 06-14 已窄，但仍是多消费点共享的解释层。
- New/Residual: context discovery policy 现在没有通过独立 typed contract 表达，而是回写到 generic mount metadata/provider string 判定，并且会影响 skill/memory runtime projection。

### Code Patterns

- `SessionRuntimeToolComposer::build_tools` 顺序调用多个 `RuntimeToolProvider` 并直接 `extend` 工具列表（`crates/agentdash-application/src/runtime_tools/provider.rs:64`）。
- VFS provider 只从 `ExecutionContext.session.vfs` 构建 `SharedRuntimeVfs`，再调用 `VfsToolFactory::build_tools`（`crates/agentdash-application/src/runtime_tools/vfs_provider.rs:61`、`:76`）。
- VFS tool factory 用 `CapabilityState.is_capability_tool_enabled` 控制 `mounts_list`、`fs_read`、`fs_glob`、`fs_grep`、`fs_apply_patch`、`shell_exec` 是否暴露（`crates/agentdash-application-vfs/src/tools/factory.rs:46`、`:52`、`:102`、`:118`）。
- `mounts_list` 暴露所有当前 session mount 及其 mount capabilities（`crates/agentdash-application-vfs/src/tools/mounts.rs:51`、`:62`）。
- `apply_agent_vfs_access_grants` 只裁剪被 `agentdash_project_vfs_mount=true` 标记的 project VFS mount；非 project VFS mount 直接跳过（`crates/agentdash-application-vfs/src/mount_project.rs:136`、`:144`）。
- owner/purpose projection 通过 provider string 和 metadata key 推断（`crates/agentdash-application-vfs/src/mount.rs:46`、`:92`）。
- skill discovery provider 的 VFS rules 会通过 VFS 扫描进入 `CapabilityState.skill.skills`（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:96`、`:130`、`:145`；`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:555`、`:567`）。
- memory discovery provider 的 VFS rules 会通过 VFS 扫描进入 memory inventory（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:208`、`:223`、`:239`；`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:570`、`:576`）。
- built-in guideline discovery 当前没有运行：`derive_runtime_guidelines` 明确返回空并记录需要 composition owner 注入 guideline providers（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:193`、`:199`、`:205`）。因此本轮不把 AGENTS.md discovery 判为已反向塑造 VFS 的现状问题。

## Issues

### 1. P1 - Context file discovery policy 借用 mount metadata/provider string，且会影响 runtime skill/memory surface

- 分类: 职责漂移 / 抽象泄漏 / 重复事实源。
- 代码证据:
  - `discover_mount_files` 对所有 mount 先调用 `should_scan_mount_for_discovery`，再按 Read/List mount capabilities 扫描规则文件（`crates/agentdash-application/src/context/mount_file_discovery.rs:103`、`:110`、`:121`、`:136`）。
  - `should_scan_mount_for_discovery` 读取 generic metadata key `agentdash_auto_discovery` / `agentdash_discovery_policy`，否则按 provider string 白名单允许 `relay_fs`、`inline_fs`、`lifecycle_vfs`、`canvas_fs`、`skill_asset_fs`（`crates/agentdash-application/src/context/mount_file_discovery.rs:312`、`:325`、`:337`）。
  - dynamic skill provider 的 rules 进入 `discover_skill_vfs_files`，随后 provider 输出被转成 session skill baseline（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:130`、`:145`、`:150`、`:187`）。
  - memory provider 的 rules 同样走 `discover_memory_vfs_files` 并输出 `MemoryDiscoveryOutput`（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability_projection.rs:223`、`:239`、`:273`）。
  - Frame construction 把 skill projection 写入 `capability_state.skill.skills`（`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:555`、`:567`），并把 memory inventory 带入 launch extras（`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:570`、`:576`; `crates/agentdash-application/src/frame_construction/assembly.rs:371`、`:374`）。
- 为什么是架构问题:
  - VFS mount 本应表达 address/provider/root/capabilities/metadata；context discovery 的成本策略、默认扫描意图和 provider allowlist 属于 context/capability projection，而不是 VFS address 模型。
  - 当前规则迫使 mount metadata 承载 discovery policy，否则 context 模块用 provider string 自行解释。新增 provider 时，开发者需要同时理解 VFS provider、mount metadata、skill/memory discovery 的隐式关系。
  - 这不是纯诊断字段：skill discovery 会改变 `CapabilityState.skill`，memory discovery 会改变 session memory frame，因此 discovery policy 已进入 runtime surface。
- 影响面:
  - VFS provider 增加或 provider 重命名会改变 discovery 默认行为。
  - 同一 mount 是否可读、是否可被 Agent 通过 `fs_read` 访问、是否可被 session 构建阶段自动扫描，是三套语言：mount capability、runtime tool capability、discovery metadata/provider allowlist。
  - 高成本/外部 provider 默认不扫的规则在 context 模块内硬编码，而不是由 provider registry 或 owner projection 以 typed policy 暴露。
- 建议收束边界:
  - 保留 mount capabilities 作为 provider operation affordance，不让 generic mount metadata 承载 context discovery policy。
  - 将 discovery policy 收束为 typed runtime projection，例如 `RuntimeDiscoveryPolicy { mount_id, allow_guidelines, allow_skills, allow_memory, cost_class }`，由 frame construction / provider registry / owner projector 生成。
  - Skill/memory providers 仍声明“找什么”，composition owner 决定“哪些 mount 可被自动扫描”；不要让 context discovery 反向要求每个 mount 写 metadata key。
- 06-14 对照: 06-14 主要指出 context 与 runtime tool 分叉、VFS mount metadata raw JSON 扩散。本问题是收束后残留的新形态：provider/tool composer 已拆开，但 context discovery policy 仍寄生在 mount metadata/provider string 上。

### 2. P1 - Mount access、Agent VFS grant、runtime tool capability 是三套并列授权语言，边界需要收窄命名

- 分类: 重复事实源 / 概念分叉 / mount ownership 边界不清。
- 代码证据:
  - `apply_agent_vfs_access_grants` 只处理 `is_project_vfs_mount(mount)`；没有 grant 的 project VFS mount 会清空 capabilities 并被移除，非 project VFS mount 完全跳过（`crates/agentdash-application-vfs/src/mount_project.rs:136`、`:144`、`:148`、`:161`）。
  - `is_project_vfs_mount` 依赖 metadata bool `agentdash_project_vfs_mount`（`crates/agentdash-application-vfs/src/mount.rs:121`）。
  - Frame construction 只在 Project owner 下应用 agent VFS grants，并且会先追加 agent knowledge mount，再裁剪 project VFS mounts（`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:363`、`:366`、`:369`）。
  - selected companion ProjectAgent 也会把 preset `vfs_access_grants` 应用到 prepared VFS（`crates/agentdash-application/src/frame_construction/request_assembler.rs:298`、`:299`、`:302`）。
  - runtime tool visibility 由 `CapabilityState` 控制工具级暴露（`crates/agentdash-application-vfs/src/tools/factory.rs:46`、`:52`、`:102`、`:118`），而 `mounts_list` 会把当前所有 mount capabilities 暴露给 Agent（`crates/agentdash-application-vfs/src/tools/mounts.rs:51`、`:62`）。
  - `CapabilityState.is_capability_tool_enabled` 是工具/capability 维度，不知道具体 mount owner 或 mount id（`crates/agentdash-spi/src/connector/mod.rs:409`、`:422`、`:433`）。
- 为什么是架构问题:
  - `AgentVfsAccessGrant` 名字看起来像通用 VFS 授权，但实际只裁剪 project VFS mounts；workspace、lifecycle、routine、canvas、skill-asset、agent knowledge mount 的可见性由各自 surface projector 决定。
  - runtime `file_read` / `file_write` grant 控制的是工具暴露，不控制具体 mount；具体 mount access 又由 `Mount.capabilities` 和 `AgentVfsAccessGrant` 共同决定。
  - mount owner/purpose 又通过 provider string + metadata 推断（`crates/agentdash-application-vfs/src/mount.rs:46`、`:92`），这让“谁拥有 mount、谁能授权 mount、Agent 看到什么工具”分散在多个层。
- 影响面:
  - 后续如果 PermissionGrant 要表达“允许读某个 mount/path”，当前没有单一落点：可以改 tool capability、改 mount capabilities、改 agent preset VFS grants，或者改 surface projector。
  - 新 provider 容易误以为 `AgentVfsAccessGrant` 会统一裁剪所有 VFS mount；实际只有 project VFS mount 会被裁剪。
  - `mounts_list` 暴露的 capabilities 是 mount operation affordance，不等同于 PermissionGrant/admission；UI/Agent 文案如果把它理解为授权结果，会与 AgentRun admission 边界冲突。
- 建议收束边界:
  - 如果当前语义正确，应把 `AgentVfsAccessGrant` 收窄命名为 Project VFS mount grant，并在类型层只接受 project VFS mount id。
  - 如果希望未来表达通用 VFS 授权，应新增独立 VFS access policy projection，由 AgentRun effective capability/admission 或 frame construction 产出 per-mount/per-path access view；`Mount.capabilities` 保留 provider 能力，不承载授权事实。
  - `mount_owner_kind` / `mount_purpose` 应继续向 typed runtime mount metadata 收束，减少 provider string 和 metadata bool 作为 owner/authorization 分支条件。
- 06-14 对照: 06-14 指出 mount metadata raw JSON 和 owner/purpose 扩散。本轮看到文件已拆分且 VFS hard validate 已补强，但 project-only grant 与 generic mount capability/capability-state 的边界仍需要命名或模型收束。

### 3. P2 - Runtime tool composer 已拆分，但 callable tool 去重/冲突检测只覆盖 schema，不覆盖执行工具

- 分类: 路径冗余 / 装配层噪音 / 潜在重复工具暴露。
- 代码证据:
  - `SessionRuntimeToolComposer::build_tools` 顺序 `extend(provider.build_tools(...))`，没有检查 tool name 重复（`crates/agentdash-application/src/runtime_tools/provider.rs:64`、`:66`）。
  - tool assembly 同样把 runtime provider tools 和 MCP tools 追加进 `all_tools`，只对 `all_schemas` 调用 `dedupe_tool_schemas`（`crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:27`、`:30`、`:75`、`:86`）。
  - `dedupe_tool_schemas` 的 key 是 source/tool_path/name；它不处理 callable `DynAgentTool` 列表（`crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:93`、`:98`）。
  - Agent runtime 执行时按 `current_tools.iter().find(|t| t.name() == tc.name)` 找第一个同名工具（`crates/agentdash-agent/src/agent_loop/tool_call.rs:351`、`:352`）。
  - 另一个 `ToolRegistry` 实现按 name 插入 HashMap，会覆盖同名工具（`crates/agentdash-agent/src/tools/registry.rs:29`、`:30`），说明不同路径对重复名称的语义并不一致。
- 为什么是架构问题:
  - composer 拆分后 provider 边界更清楚，但工具名唯一性这个 composition invariant 没有成为 composition root 的显式检查。
  - schema surface 与 callable surface 可能产生不同结果：schema 已去重，执行工具未去重；不同调用路径对重复工具名是 first-wins 或 last-wins。
- 影响面:
  - VFS/workflow/collaboration/task/workspace-module/MCP 任一 provider 引入同名工具时，最终可见 schema、agent 可调用工具和审计/usage projection 可能不一致。
  - 这不是当前已观察到的重复工具名 bug；它是 composer 拆分后缺少唯一性 guard 的 P2 风险。
- 建议收束边界:
  - `SessionRuntimeToolComposer` 或 `assemble_tool_surface_for_execution_context` 应统一校验 callable tool name 唯一性，重复时返回配置错误或带 provider/source 的诊断。
  - schema dedupe 和 callable tool dedupe 使用同一 source/tool_path/name 语义；对 Agent 可调用的裸 `name` 必须仍保持全局唯一。
  - 新 provider 注册时应在 provider 单元测试和 bootstrap integration test 中断言 tool name 不冲突。
- 06-14 对照: 06-14 的跨域 composer 职责漂移已解决；本问题是拆分后 composition root 需要补的唯一性 invariant，不是旧问题复发。

## Not Problems / Resolved Boundaries

- `VfsRuntimeToolProvider` 当前没有继续组装 workflow/collaboration/workspace module tools；06-14 的 `RelayRuntimeToolProvider` 职责漂移已解决。
- `VfsToolFactory` 的职责仍窄：只组装 mounts/fs/shell，并按 `CapabilityState` gating 逐个工具暴露。
- `validate_vfs` 已在主要 VFS builder 路径执行，覆盖 mount id、default mount、provider/root_ref/capability/link 等 hard validation；本轮未发现“构建后完全不 validate”的主路径。
- built-in `AGENTS.md` guideline discovery 当前未进入 AgentRun projection；`derive_runtime_guidelines` 返回空。因此本轮不能把 guideline discovery 作为已经反向塑造 VFS 的现状问题，只能记录为未接入/待 composition owner 决策。

## External References

- 未使用外部资料。本轮为仓库内部只读架构审查。

## Related Specs

- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/cross-layer/architecture.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件按用户显式指定的 `.trellis/tasks/06-30-module-adversarial-review/research/09-vfs-runtime-tool-surface.md` 写入。
- 未修改业务代码，未运行全量测试，符合本轮只读审查范围。
- 未发现当前 VFS provider 再次漂移为跨域 runtime tool composer；该 06-14 baseline 问题已收束。
- 未发现 built-in guideline discovery 当前实际注入 `system_guidelines`；代码中保留规则定义和 frame 字段，但 AgentRun projection 返回空。
- 未验证所有 provider 运行时是否存在真实重复 tool name；P2 工具去重结论基于 composition root 缺少唯一性 guard，而不是已观测冲突实例。
