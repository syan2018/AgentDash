# Research: Project / Workspace / Backend Placement + Knowledge & Context Surface

- Query: 对抗性架构审查 Project / Workspace / Backend Placement + Knowledge & Context Surface，重点覆盖 project/workspace/backend/local runner enrollment/machine and workspace identity/settings，以及 skill assets/shared library/context construction/MCP presets/story and session context。
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `.trellis/tasks/06-30-module-adversarial-review/check.jsonl` - 当前任务检查点和审查上下文。
- `.trellis/tasks/06-30-module-adversarial-review/prd.md` - 当前任务目标和范围。
- `.trellis/tasks/06-30-module-adversarial-review/design.md` - 当前任务的设计约束。
- `.trellis/tasks/06-30-module-adversarial-review/implement.md` - 当前任务执行记录。
- `.trellis/tasks/06-30-module-adversarial-review/brief-review-placement-context.md` - 本轮 Placement + Context Surface 审查 brief。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - ProjectBackendAccess、BackendWorkspaceInventory、WorkspaceBinding 与 execution lease 的边界规范。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - desktop/local runner/machine identity/enrollment 边界规范。
- `.trellis/spec/backend/embedded-skill-bundles.md` - system skill 通过 Project SkillAsset 与 lifecycle projection 暴露的规范。
- `.trellis/spec/backend/shared-library.md` - Shared Library 与 Project installed asset 的运行时边界规范。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 旧 baseline 汇总。
- `.trellis/tasks/06-14-module-overdesign-review/research/02-agentrun-session-runtime.md` - RuntimeGateway/SessionRuntimeInner baseline。
- `.trellis/tasks/06-14-module-overdesign-review/research/03-vfs-local-relay-extension.md` - VFS/local relay/extension placement baseline。
- `.trellis/tasks/06-14-module-overdesign-review/research/04-frontend-contracts-permission.md` - frontend/contracts/permission baseline，仅作链路对照。
- `crates/agentdash-application/src/backend/runner_registration.rs` - runner registration token claim 用例。
- `crates/agentdash-application/src/backend/management.rs` - local backend enrollment、stable backend id、auth token 生成。
- `crates/agentdash-application/src/backend/project_access.rs` - ProjectBackendAccess grant upsert/reactivation。
- `crates/agentdash-application/src/workspace/backend_sync.rs` - workspace detect 结果到 BackendWorkspaceInventory + WorkspaceBinding 的统一 fact。
- `crates/agentdash-application-runtime-session/src/backend_execution_placement.rs` - backend execution placement/lease 选择。
- `crates/agentdash-application/src/relay_connector.rs` - relay prompt 要求 session backend_execution，并用 backend_id + lease_id 注册路由。
- `crates/agentdash-local-tauri/src/main.rs` - desktop profile、desktop access-token claim、runtime start request。
- `crates/agentdash-local/src/machine_identity.rs` - machine identity 的本地库事实源。
- `crates/agentdash-local/src/runner_claim.rs` - headless runner claim/config 路径。
- `crates/agentdash-application-lifecycle/src/lifecycle/surface/surface_projector.rs` - BuiltinLifecycleSkill policy 和 lifecycle skill projection。
- `crates/agentdash-application-skill/src/skill_asset/definition.rs` - builtin SkillAsset template 注册，包括 memory-manager。
- `crates/agentdash-application-skill/src/skill_asset/service.rs` - builtin SkillAsset bootstrap 到 Project。
- `crates/agentdash-application-vfs/src/mount_skill_asset.rs` - lifecycle mount metadata 中的 skill_asset_keys 投影。
- `crates/agentdash-application-vfs/src/provider_skill_asset.rs` - lifecycle projected skill files 到 `skills/<key>/...` 的读取。
- `crates/agentdash-application-vfs/src/mount_project.rs` - ProjectAgent knowledge mount 构造。
- `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs` - Project owner VFS、builtin skill、capability/MCP/context bootstrap。
- `crates/agentdash-application/src/frame_construction/request_assembler.rs` - companion/session request capability 和 MCP assembly。
- `crates/agentdash-application/src/frame_construction/mod.rs` - FrameLaunchEnvelope closure 与 runtime backend anchor 派生。
- `crates/agentdash-application/src/capability/resolver.rs` - tool capability reduction 与 MCP preset resolution。
- `crates/agentdash-application/src/mcp_preset/runtime.rs` - MCP runtime_binding source 读取和 RuntimeMcpServer materialization。
- `crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs` - FrameLaunchSurface 一致性校验与 VFS-derived runtime backend anchor。
- `crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs` - CapabilityState 到 VFS/MCP frame surfaces 的同步拆分。
- `packages/app-web/src/features/project/agent-preset-editor/knowledge-section.tsx` - ProjectAgent knowledge UI 链路证据，不作为独立模块审查。

### Code patterns

- Runner enrollment 已把 backend identity 从 project 中解耦。`runner_registration.rs:234-273` 明确 runner identity 不再携带 project，project 可见性通过 `ProjectBackendAccess` 赋权；随后调用 `enroll_local_backend` 和 `ensure_project_backend_access_grant`。
- `management.rs:228-264` 的 stable backend id 输入为 `machine_id/share_scope/capability_slot`，注释明确不包含 project id；`management.rs:241-249` 把 runner token 转为 user-scope shared backend；`management.rs:291-294` 统一走 `ensure_local_backend`。
- `project_access.rs:48-129` 将 project/backend grant 作为独立授权事实，支持 active row reactivation 和 create conflict 后重查。
- `workspace/backend_sync.rs:38-95` 从同一个 detect result 生成 `WorkspaceDirectoryFact { binding, inventory }`；`workspace/backend_sync.rs:118-146` 一次性 upsert binding、refresh status/default binding；`workspace/backend_sync.rs:177-255` 只在 active ProjectBackendAccess backend 集合内同步 workspace facts。
- `backend_execution_placement.rs:1-213` 把 execution placement 表达为 explicit/auto-idle/workspace-binding intent，并用 backend execution lease repo 做 active count 排序。
- `relay_connector.rs:103-109` 要求 `context.session.backend_execution`，并使用其中的 `backend_id` 和 `lease_id`；`relay_connector.rs:164-180` 注册 relay route 并在成功 prompt 后 activate lease。
- `agentdash-local-tauri/src/main.rs:109-145` 在 Tauri shell 内定义 `RuntimeStartRequest` 和 `LocalRuntimeProfile`；`main.rs:244-263` 直接读写 profile；`main.rs:638-757` 在 Tauri shell 内执行 local runtime claim HTTP payload、retry、validation 和 `/api/local-runtime/ensure` 调用；`main.rs:788-830` 在 shell 内 normalize profile/start request，并调用 `load_or_create_machine_identity`。
- `surface_projector.rs:141-187` 定义 `BuiltinLifecycleSkill` 和 policy；当前 enum 包含 canvas-system、companion-system、workspace-module-system、routine-memory。
- `surface_projector.rs:424-446` 的 builtin bootstrapper 通过 `SkillAssetService::bootstrap_builtins(project_id, Some(builtin_key))` 把 builtin 先变成 Project SkillAsset。
- `surface_projector.rs:521-545` 在 `EnsureAndProject` 下 bootstrap builtin skill 后把 skill key 加入有效投影；`surface_projector.rs:610-680` 刷新 lifecycle mount projection metadata。
- `mount_skill_asset.rs:38-140` 将 lifecycle mount metadata 中的 `skill_asset_keys` 作为投影事实维护；`provider_skill_asset.rs:66-131` 根据 metadata 读取 Project SkillAsset 文件并暴露为 `skills/<key>/<file>`。
- `skill_asset/definition.rs:15-40` 注册 builtin templates，其中包括 `memory-manager`；但 `rg memory-manager` 仅在 definition/tests 层命中，没有进入 lifecycle builtin enum 或 ProjectAgent knowledge bootstrap 路径。
- `mount_project.rs:97-116` 仅在 `ProjectAgent.knowledge_enabled` 时追加 `agent` mount；`mount_project.rs:236-270` 把该 mount 定义为 inline context container，具备 read/write/list/search。
- `owner_bootstrap.rs:363-368` 在 Project owner 且无 existing VFS 时追加 ProjectAgent knowledge mount；`owner_bootstrap.rs:394-401` 同一路径只确保 companion/canvas/workspace-module 三个 builtin lifecycle skills，不含 memory-manager。
- `capability/resolver.rs:308-310` 在遇到 custom MCP capability 时调用 `resolve_preset_mcp_server(preset, input.mcp_runtime_context.as_ref())`，错误会上抛到 `resolve_checked`。
- `owner_bootstrap.rs:534-536` 和 `request_assembler.rs:423-425` 传入 `McpRuntimeBindingContext { vfs, backend_anchor: None }`。
- `mcp_preset/runtime.rs:127-136` 对 required 的 `MissingRuntimeBackendAnchor` 返回错误；`runtime.rs:166-169` 的 `RuntimeBackendAnchorBackendId` source 从 `context.backend_anchor` 读取。
- `frame_construction/mod.rs:515-524` 在 closed launch surface 之后才从 `FrameLaunchSurface.runtime_backend_anchor(...)` 派生 runtime backend anchor。
- `frame_construction/mod.rs:589-599` 在 closure 后把 effective MCP servers 写回 capability state 和 surface draft；`runtime_launch.rs:145-147` 对 `capability_state.tool.mcp_servers` 与独立 mcp surface 做一致性校验。

### Issue P1: Desktop local runtime shell still owns profile and desktop claim protocol

- Baseline: `residual` from 06-14 `03-vfs-local-relay-extension.md` 的 Tauri profile/claim duplication 问题。服务端 enrollment identity 已收束，但 desktop shell 仍保留一段本应属于 local runtime library/enrollment client 的协议逻辑。
- Classification: module boundary leak / duplicated enrollment client / settings ownership drift。
- Code evidence:
  - `crates/agentdash-local-tauri/src/main.rs:109-145` 定义 `RuntimeStartRequest`、`LocalRuntimeProfile`，包括 server_url/access_token/profile_id/machine_id/workspace_roots/executor_enabled 等运行时设置。
  - `crates/agentdash-local-tauri/src/main.rs:244-263` 的 `profile_load/profile_save` 直接读写并 normalize desktop profile。
  - `crates/agentdash-local-tauri/src/main.rs:638-757` 的 `start_runtime_from_request/claim_local_runtime/post_local_runtime_claim` 直接构造 `EnsureLocalRuntimePayload`、重试 API、校验 response、调用 `/api/local-runtime/ensure`。
  - `crates/agentdash-local-tauri/src/main.rs:788-830` 的 `normalize_profile/normalize_start_request` 直接调用 local library 的 `load_or_create_machine_identity`，说明 machine identity 已被下沉，但 profile 和 claim 协议仍留在 shell。
  - `crates/agentdash-local/src/machine_identity.rs` 已是 machine identity 的本地库事实源；`crates/agentdash-local/src/runner_claim.rs` 另有 runner token claim 路径，形成 desktop access-token claim 与 runner claim 两条本地 enrollment client。
- Impact:
  - Desktop 与 headless runner 在 payload 字段、profile 持久化、token 清理、capability_slot/share_scope/registration_source 校验上容易独立漂移。
  - settings 层会把 UI shell、local runtime library、server enrollment use case 三方绑在一起；未来扩展 backend share scope、capability slot 或 executor config 时，需要同步改多处。
  - 该问题不再影响 server 端 ProjectBackendAccess 权威性，但仍影响 local runtime client 的长期可维护性。
- Suggested boundary:
  - `agentdash-local` 应拥有 local runtime profile DTO、profile path/read/write/normalize、desktop access-token claim client、response validation 和 redaction 规则。
  - `agentdash-local-tauri` 只保留 Tauri command adapter、desktop API lifecycle、窗口/状态事件桥接；它传入用户输入并调用 local library，不能自己构造 enrollment HTTP protocol。
  - Headless runner token claim 与 desktop access-token ensure 可以是同一 local enrollment client 的两个 auth strategy，而不是两个调用端各自拼 payload。
- Priority: P1.

### Issue P1: `memory-manager` builtin is not tied to ProjectAgent knowledge lifecycle projection

- Baseline: `resurfaced` as a system skill projection gap. 06-14 baseline 未单独覆盖 memory-manager，但同类问题已经由当前 spec 收束为“system Skills 必须先成为 Project-level builtin SkillAsset，再通过 lifecycle projection 暴露”。
- Classification: concept split / duplicated source of truth / context protocol leak。
- Code evidence:
  - `crates/agentdash-application-skill/src/skill_asset/definition.rs:37` 注册 builtin template `memory-manager`。
  - `crates/agentdash-domain/src/agent/value_objects.rs` 定义 memory manager skill name/path/bundle，skill 文本引用 `agent://MEMORY.md` 等 ProjectAgent knowledge surface。
  - `crates/agentdash-application-vfs/src/mount_project.rs:97-116` 仅根据 `ProjectAgent.knowledge_enabled` 追加 `agent` knowledge mount；`mount_project.rs:236-270` 构造 `context://inline/knowledge` mount。
  - `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:363-368` 将 ProjectAgent knowledge mount 并入 Project owner VFS。
  - `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:394-401` 同一路径只 `EnsureAndProject` companion-system、canvas-system、workspace-module-system，不包括 memory-manager。
  - `crates/agentdash-application-lifecycle/src/lifecycle/surface/surface_projector.rs:141-187` 的 `BuiltinLifecycleSkill` enum 不含 MemoryManager；`rg memory-manager` 未发现 application/api/frontend 的 lifecycle projection 调用点。
  - `packages/app-web/src/features/project/agent-preset-editor/knowledge-section.tsx:51-61` 仅作为链路证据：UI 操作的是 `project_agent_knowledge` 的 `agent` mount，而不是 skill 投影权威。
- Impact:
  - `knowledge_enabled` 只打开了 VFS/storage fact，没有同步投影“如何维护 agent knowledge”的 instruction/protocol fact。
  - Agent 可以看到 `agent://` mount，但是否获得 memory-management 操作约束取决于用户是否额外选择 `memory-manager` skill asset，破坏了 knowledge feature 的单一开关语义。
  - `memory-manager` 作为 builtin template 存在，却不参与 lifecycle builtin policy，会让 Shared Library/SkillAsset/Context Surface 三层对同一系统能力的归属不一致。
- Suggested boundary:
  - 将 `MemoryManager` 纳入 `BuiltinLifecycleSkill`，并在 Project owner + `ProjectAgent.knowledge_enabled` 为 true 时与 `append_agent_knowledge_mounts` 同步 `EnsureAndProject`。
  - 如果 `memory-manager` 与 `RoutineMemory` 是同一协议，应在 domain 层合并成一个明确 builtin key；如果不是，应保持 ProjectAgent knowledge 与 Routine memory 两条 lifecycle builtin，但不要让 UI skill picker 成为系统 memory protocol 的事实源。
  - 继续保持内容来源为 Project SkillAsset，不要绕过 SkillAsset 直接塞 prompt/context。
- Priority: P1.

### Issue P1: MCP runtime binding asks for backend anchor before backend anchor exists

- Baseline: `resurfaced` after placement refactor. 06-14 已指出 runtime/local relay/VFS placement 需要避免横向猜测；当前 execution placement 已解决，但 MCP preset runtime binding 又在 context construction 顺序上重新暴露 VFS-derived backend anchor 与 MCP surface 的时序裂缝。
- Classification: construction order drift / cross-surface coupling / latent runtime preset failure。
- Code evidence:
  - `crates/agentdash-application/src/capability/resolver.rs:308-310` 在 capability resolution 阶段把 MCP preset materialize 成 `RuntimeMcpServer`，错误通过 `resolve_checked` 返回。
  - `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:534-536` 给 owner capability resolution 的 `McpRuntimeBindingContext` 传入 `backend_anchor: None`。
  - `crates/agentdash-application/src/frame_construction/request_assembler.rs:423-425` 给 companion selected project agent capability resolution 也传入 `backend_anchor: None`。
  - `crates/agentdash-application/src/mcp_preset/runtime.rs:127-136` 对 required 的 missing runtime backend anchor 返回 `McpRuntimeBindingError::MissingRuntimeBackendAnchor`；`runtime.rs:166-169` 的 `RuntimeBackendAnchorBackendId` source 只能从 `context.backend_anchor` 读取。
  - `crates/agentdash-application/src/frame_construction/mod.rs:515-524` 在 closed surface 后才从 launch surface 派生 `runtime_backend_anchor`。
  - `crates/agentdash-application/src/frame_construction/mod.rs:589-599` closure 后只是把 pending runtime command 的 effective MCP servers 写回；常规 MCP preset resolution 已经在 capability 阶段发生。
  - `crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs:145-147` 校验 capability_state 中的 MCP servers 与 surface MCP servers 一致，说明 frame 内 MCP surface 是冻结事实，不是后续按 anchor 重新解析。
- Impact:
  - 任何 required `RuntimeBackendAnchorBackendId` runtime_binding 的 MCP preset，在 Project owner 或 companion child 的初始 frame construction 中都会缺少 backend anchor，即使最终 VFS default mount 足以派生出该 backend anchor。
  - 结果可能是 frame construction 失败，或调用方改用非严格/optional binding 后静默缺字段；两者都会让 Project/Workspace/backend placement 的权威结果无法稳定流入 MCP server transport。
  - 这不是单个 MCP preset 的配置问题，而是 context surface construction 顺序和 ownership 问题。
- Suggested boundary:
  - 在 VFS 已完成 owner/workspace/lifecycle/knowledge closure 后，先派生唯一 `RuntimeBackendAnchor`，再调用 `CapabilityResolver` materialize MCP presets，并把 anchor 传入 `McpRuntimeBindingContext`。
  - 或者如果该 source 的第一性事实就是 VFS default mount backend id，则将 binding source 明确改名/改语义为 `VfsDefaultMountBackendId`，去掉“后派生 RuntimeBackendAnchor 再回头使用”的顺序错位。
  - MCP surface、CapabilityState.tool.mcp_servers、FrameLaunchSurface 的一致性校验可以保留，但前置事实源必须只有一个。
- Priority: P1.

### Resolved / acceptable boundaries

- Runner registration token enrollment is resolved vs 06-14 baseline. `runner_registration.rs:234-273` + `management.rs:228-264` + `project_access.rs:48-129` 表明 runner backend identity 已按 machine/scope/slot 稳定，ProjectBackendAccess 是 project 可见性的权威事实；旧的“backend id/project id 混合”问题为 `resolved`。
- Workspace directory facts are resolved vs 06-14 baseline. `workspace/backend_sync.rs:38-146` 把 detect result 同步成 inventory + binding 的同一目录事实，且 `workspace/backend_sync.rs:177-255` 只在 active project-backend access 内同步；目录事实未再承担 executor idle/busy。
- Execution placement is resolved vs 06-14 baseline. `backend_execution_placement.rs:1-213` 使用 backend execution lease/availability；`relay_connector.rs:103-180` 要求 backend_execution 并使用 lease route，未从 VFS mount 反推 execution backend。
- VFS-derived runtime backend anchor remains acceptable if it stays a launch-surface anchor. `runtime_launch.rs:197-224` 和 `frame_construction/mod.rs:515-524` 从 final VFS default mount 派生 runtime backend anchor；只要 execution lease 仍由 backend execution placement 决定，这不是旧问题复发。上面的 MCP issue 针对的是 MCP preset resolution 的时序，而不是 anchor 派生本身。
- Canvas/companion/workspace-module system skill projection is resolved. `surface_projector.rs:424-545` + `mount_skill_asset.rs:38-140` + `provider_skill_asset.rs:66-131` 表明主要 system skill 已走 Project SkillAsset -> lifecycle mount projection -> provider file surface；旧的“直接把系统 skill 当 context 元数据塞入”问题在这些路径上为 `resolved/superseded`。
- CapabilityState 与独立 VFS/MCP surfaces 的重复存储有一致性闸门。`runtime_capability.rs:82` 拆分 surfaces；`runtime_launch.rs:145-147` 校验 MCP servers 一致；`frame_construction/mod.rs:589-599` 在 runtime command closure 后写回。这里是冗余投影，不是本轮独立 issue。

### Related specs

- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` 要求 `ProjectBackendAccess` 是 project->backend grant 权威，workspace detect 同步 inventory/binding，execution placement 由 backend execution lease/allocator 拥有。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` 要求 `agentdash-local` 拥有 machine identity，本地 runtime/runner 领取使用 server claim result，Tauri shell 应保持薄适配。
- `.trellis/spec/backend/embedded-skill-bundles.md` 要求 system Skills 先成为 Project-level builtin `SkillAsset`，再投影为 lifecycle mount `skills/<key>/...`。
- `.trellis/spec/backend/shared-library.md` 要求 runtime 路径读取 Project installed resources，而不是直接读取 Shared Library/LibraryAsset payload。
- `.trellis/spec/cross-layer/architecture.md` 支撑本轮使用第一性原理审查 module boundary、fact ownership 和 projection surface。

### External references

- None. 本轮为 internal code/spec review，未使用外部文档或联网资料。

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回当前 task 为 none；本文件使用用户显式提供的 task path 写入。
- 未运行全量测试，符合用户要求；本轮只读业务代码并只写入本 research 文件。
- 前端 feature/store/API route/generated contract 仅作为 ProjectAgent knowledge 或链路证据引用，没有作为独立模块泛化审查。
- 未发现 runner token enrollment 在服务端继续以 project id 生成 backend identity；该旧 baseline 问题按当前证据为 resolved。
- 未发现 workspace binding/inventory 仍承担 execution idle/busy 语义；execution placement 证据指向 backend execution lease。
