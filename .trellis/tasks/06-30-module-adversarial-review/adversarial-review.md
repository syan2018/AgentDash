# 对抗性模块架构审查综合报告

## 总结

本轮 review 的主要结论是：06-14 的若干高风险问题已经收束，尤其是 lifecycle cancel、Task runtime projection、RuntimeSession runtime-control、runtime tool composer、local command router、extension host schema/workspace root、permission typed contract 等主链路。但新的残留集中在更靠近边界的地方：

- 授权与能力运行态边界仍有 P0：tool-level PermissionGrant 被写回 `CapabilityState`，且 runtime admission 按 run 读取 active grants，绕过 `effect_frame_id`。
- AgentRun / RuntimeSession 已移除明显重复 command surface，但 launch command、command availability、mailbox steering、delegate trait 仍有多路径。
- Orchestrated Work 主 reducer 链路健康，但 companion/routine gate 与 durable wait/read model 仍产生第二层状态语言。
- Extension / Workspace Module 大方向统一，但 Canvas promoted extension loadability、dynamic extension action discovery、invocation workspace resolver 存在分叉。
- VFS / Local / Placement / Knowledge 的主要问题不是“模块完全错位”，而是 typed metadata、profile/claim、workspace directory fact、settings、context finalization 等事实源边界未完全收束。

优先级上，建议先处理 Authority & Capability Runtime 的两个 P0，然后按 P1 owner 分组拆后续实现任务。

## Quick Convergence Status

Follow-up task `.trellis/tasks/06-30-architecture-quick-convergence/` completed the bounded quick/medium cleanup set:

- Issue 1/2: tool-level grant no longer expands visible `CapabilityState`; runtime projection uses frame-scoped grants. The remaining production execution guard is tracked in D1.
- Issue 9: delegate and scheduler steering now share one delivery executor.
- Issue 11/13/14: extension loadability, workspace resolver, and schema validation have a shared projection/validator/resolver path.
- Issue 20: legacy `user_preferences` business consumption was migrated to scoped settings and the backend preference port was removed.
- Issue 22/24/25/26: builtin VFS skill identity, runtime tool name uniqueness, handler-declared local relay scheduling, and shared workspace root guard were implemented.

Design-class residuals remain in `followups/design-backlog.md`; the quick task intentionally did not implement owner changes such as RuntimeGateway dynamic action discovery, VFS per-mount/path authorization, AgentRuntimeDelegate split, or the full AgentRun effective/admission production boundary.

## P0

### 1. Tool-level PermissionGrant 被写回 CapabilityState，混淆模型可见能力与执行准入

- Owner：Authority & Capability Runtime。
- 类型：重复事实源 / 授权边界污染 / availability 与 admission 混用。
- Baseline：06-14 的 companion grant 双事实源已被阻断，但问题迁移为 runtime capability state 污染。
- 证据：
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:37` 注释声明 tool-level grant 只应作为执行准入。
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:116` 的 `apply_to_execution_capability_state` 把 admission projection 写入 `CapabilityState`，扩展 capability、enabled cluster 与 tool policy。
  - `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:194` 在装配工具前替换 `context.turn.capability_state`。
  - `crates/agentdash-application-vfs/src/tools/factory.rs:46` 起各 VFS tool factory 使用 `CapabilityState` 决定工具是否暴露。
  - `crates/agentdash-application-ports/src/agent_run_surface.rs:309` 已有 `AgentRunEffectiveCapabilityPort::admit_tool`，但产品路径未使用该 admission boundary。
- 影响：
  - tool-level grant 本应只影响执行准入，却可能扩大模型可见 tool surface。
  - VFS/MCP/workspace module/workflow provider 看到的是被 grant 改写后的 `CapabilityState`，无法区分可见能力与临时准入。
- 建议：
  - `CapabilityResolver` / AgentFrame surface 只表达 declarative visible capability。
  - PermissionGrant tool-level grant 只进入 AgentRun admission decision。
  - tool schema exposure 消费 final visible capability view；tool invocation 消费 `AgentRunEffectiveCapabilityPort::admit_tool`。

### 2. Runtime admission projection 按 run 读取 active grants，绕过 effect_frame_id

- Owner：Authority & Capability Runtime。
- 类型：授权作用域泄漏 / frame boundary bypass。
- Baseline：06-14 要求 PermissionGrant 成为唯一授权事实源；当前事实源唯一，但读取维度错误。
- 证据：
  - `crates/agentdash-domain/src/permission/entity.rs:17` 的 `PermissionGrant` 同时持有 `run_id`、`effect_frame_id`、`source_runtime_session_id`。
  - `crates/agentdash-domain/src/permission/repository.rs:24` 已有 `list_by_frame`，`:38` 已有 `list_active_by_frame`。
  - `crates/agentdash-application/src/permission/service.rs:24` 的 `GrantRequest` 要求 `effect_frame_id`。
  - `crates/agentdash-application-agentrun/src/agent_run/permission_runtime_surface_update.rs:130` 以 `grant.effect_frame_id` 更新 capability surface。
  - `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:318` runtime projection 调用 `list_active_by_run(anchor.run_id)`。
- 影响：
  - 同一 run 内不同 AgentFrame / agent / runtime session 的 active grant 可能互相污染。
  - 多 agent、多 frame、scope escalation 场景下授权影响面从 effect frame 扩大到整个 run。
- 建议：
  - runtime session 先解析当前 effect frame，再用 `list_active_by_frame(effect_frame_id)` 构建 admission projection。
  - run-level query 只用于 UI/audit/read model，不进入执行准入。

## P1

### 3. Companion capability grant payload 仍保留旧授权协议

- Owner：Orchestrated Work Surface / Authority & Capability Runtime。
- 类型：概念分叉 / stale authorization protocol / gate 死路。
- Baseline：06-14 P0 双事实源已降级，但旧入口仍残留。
- 证据：
  - `crates/agentdash-application/src/companion/payload_types.rs:87` 注册 `capability_grant_request`。
  - `crates/agentdash-application/src/companion/payload_types.rs:283` 使用旧 scope：`turn | session | workflow_step`。
  - `crates/agentdash-contracts/src/system/permission.rs:6` / `crates/agentdash-domain/src/permission/value_objects.rs:8` 的正式 scope 是 `turn | agent_frame | activity`。
  - `crates/agentdash-application/src/companion/tools.rs:2006` 明确该 platform broker 不存在。
  - `packages/app-web/src/features/session/model/companionRequestViewModel.ts:42` 仍映射 capability grant card。
- 影响：不会落假授权事实，但可以创建不可闭环的人类 gate，并保留旧授权语言。
- 建议：删除该 payload，或改为 PermissionGrant request projection；human companion gate 不应接受 capability grant payload。

### 4. Routine execution history 暴露 dispatch ledger status，而非 runtime terminal status

- Owner：Orchestrated Work Surface。
- 类型：重复事实源 / 投影不完整 / 命名职责漂移。
- Baseline：新发现。
- 证据：
  - `crates/agentdash-domain/src/routine/entity.rs:204` 注释说明 terminal status 应从 LifecycleRun / Agent projection 派生。
  - `crates/agentdash-domain/src/routine/entity.rs:225` 的 `RoutineExecutionStatus` 只有 `pending / dispatched / failed / skipped`。
  - `crates/agentdash-api/src/routes/routines.rs:231` 直接映射 execution row。
  - `packages/app-web/src/features/routine/execution-history-panel.tsx:31` 直接展示 `exec.status`。
- 影响：成功/失败/取消的真实 runtime 状态不会回到 Routine history，用户长期看到 `dispatched`。
- 建议：保留 ledger status 但改名为 `dispatch_status`；新增 read model 从 lifecycle/agent/runtime node 派生 `runtime_status`。

### 5. LifecycleDispatchService 仍是过厚 transaction script

- Owner：Orchestrated Work Surface。
- 类型：模块过厚 / 横向耦合。
- Baseline：06-14 residual。
- 证据：
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:105` 的 service 持有 run、graph、agent、frame、association、gate、lineage、anchor、runtime session、frame construction、materialization、planner。
  - `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:529` 的 dispatch path 仍串联 graph planning、run/orchestration、agent、association、runtime session、frame、lineage、gate、anchor、`NodeStarted`。
- 影响：workflow graph planning、AgentRun identity、RuntimeSession delivery、gate、lineage 都共享一个修改热点。
- 建议：保留 facade，但拆内部 owner：run/orchestration starter、subject association writer、agent/frame/session materializer、lineage/gate writer、graph-backed reducer bridge。

### 6. CompanionGateControlService 混合 durable gate 状态与 delivery 机制

- Owner：Orchestrated Work Surface / Agent Runtime Session Surface。
- 类型：模块过厚 / 抽象泄漏。
- Baseline：resurfaced。
- 证据：
  - `crates/agentdash-application/src/companion/gate_control.rs:346` 同时持有 gate repo、run repo、frame/agent/anchor/lineage repo、notification、parent mailbox、human response mailbox。
  - `gate_control.rs:417` 处理 human response，`:537` 处理 child result，`:727` 处理 parent request，`:905` 处理 parent response。
  - `crates/agentdash-api/src/routes/companion_gates.rs:51` 为简单 human response 构造完整 service。
- 影响：gate 状态变更和 mailbox/session delivery 规则绑在一起，难以审计“谁改变 gate”。
- 建议：拆 `LifecycleGateResolver` 与 delivery adapters；gate transition 返回 delivery intents，由 mailbox/session adapters 消费。

### 7. Launch command identity 在 AgentRun / RuntimeSession / FrameLaunchPort 三层重复

- Owner：Agent Runtime Session Surface。
- 类型：路径冗余 / 概念分叉。
- Baseline：new/resurfaced。
- 证据：
  - `crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:159` / `:171` 定义 AgentRun launch source/command。
  - `crates/agentdash-application-runtime-session/src/session/launch/command.rs:11` / `:23` 定义 RuntimeSession 版本。
  - `crates/agentdash-application-ports/src/frame_launch_envelope.rs:127` / `:160` 定义 FrameLaunch 版本。
  - `crates/agentdash-application/src/runtime_session_agent_run_bridge.rs:202`、`crates/agentdash-application-runtime-session/src/session/launch/orchestrator.rs:89`、`crates/agentdash-application/src/frame_construction/mod.rs:285` 存在来回映射。
- 影响：新增 launch source 需要同步三层 enum 和 mapping；frame construction 看起来消费 transport DTO，而非拥有 launch-ready facts。
- 建议：保留一个 domain launch command/source model；AgentRun/RuntimeSession 只构造它，不维护并行 enum。

### 8. Command/action availability 仍有多个 derivation owner

- Owner：Agent Runtime Session Surface。
- 类型：重复投影 / command availability owner drift。
- Baseline：06-14 residual but reduced。
- 证据：
  - `agent_run/workspace/query.rs:158` 构建 `AgentRunWorkspaceProjection`，`:213` 又构建 `AgentConversationSnapshot`。
  - `agent_run/workspace/projection.rs:12` 起从 `SessionExecutionState` 派生 workspace projection。
  - `agent_run/conversation_snapshot.rs:596` / `:623` 独立派生 conversation execution 与 command enablement。
  - `agent_run/workspace/command_policy.rs:40` / `:156` 又重查并派生命令可用性。
- 影响：UI enablement 与 stale guard rejection 可能漂移；新 command 需改多处。
- 建议：`AgentConversationSnapshot` / `ConversationCommandAvailabilityResolver` 成为唯一 command availability owner；command policy 复用其 resolver 输出。

### 9. Mailbox steering 有两套消费实现，terminal/error 语义不同

- Owner：Agent Runtime Session Surface。
- 类型：路径冗余 / delivery fact drift。
- Baseline：06-14 residual。
- 证据：
  - `mailbox/scheduler.rs:297` delegate path 调 `consume_as_delegate_steering`。
  - `mailbox/scheduler.rs:556` scheduler path 调 `consume_as_steering`。
  - `mailbox/scheduler.rs:337` delegate event write failure 标记 `Failed`。
  - `mailbox/scheduler.rs:639` normal steering 可标记 `Steered` 但带 `last_error`。
- 影响：同类 mailbox delivery 根据路径得到不同 receipt/status/event 语义。
- 建议：抽一个 steering delivery executor，delegate path 只决定返回给 loop 的 output shape。

### 10. AgentRuntimeDelegate 仍过宽

- Owner：Agent Runtime Session Surface。
- 类型：过宽抽象 / 横向耦合。
- Baseline：06-14 residual。
- 证据：
  - `crates/agentdash-agent-types/src/runtime/delegate.rs:25` 一个 trait 覆盖 compaction、context transform、tool policy、after-turn、before-stop、provider observer。
  - `agent_run/mailbox_runtime_adapter.rs:357` 起 mailbox wrapper 被迫转发非 mailbox 方法。
  - `session/launch/planner.rs:146` / `:161` 通过 wrapper 顺序组合 hook 与 mailbox delegate。
- 影响：mailbox turn-boundary concern 需要知道 agent loop 的所有 extension point。
- 建议：拆成 delegate set：context transform、tool policy、compaction、turn boundary、provider observer；mailbox 只实现 turn-boundary。

### 11. Canvas promoted extension loadability 在 Extension tab 与 WorkspaceModule descriptor 分叉

- Owner：Extension / Workspace Module Runtime Surface。
- 类型：概念分叉 / 重复事实源。
- Baseline：new。
- 证据：
  - `canvas/promotion.rs:70` / `:74` / `:79` 生成 `CanvasPanel` tab，但 bundles 为空。
  - `workspace_module/mod.rs:296` / `:300` / `:303` 用 `ext.bundles` 判断 module ready。
  - `extensionTabDescriptors.tsx:17` / `:33` / `:34` 对 `canvas_panel` 直接渲染。
  - `ExtensionCanvasPanel.tsx:34` / `:39` / `:46` 从 package artifact snapshot 加载。
- 影响：用户能打开的 Canvas-derived extension，Agent 通过 workspace module catalog 可能看到 unavailable。
- 建议：extension runtime projection 产出 renderer-aware loadability；WorkspaceModule descriptor、extension tabs、management summary 共用同一 projection。

### 12. RuntimeGateway dynamic extension actions 可 invoke 但不进 surface_for_actor

- Owner：Extension / Workspace Module Runtime Surface / Agent Runtime Session Surface。
- 类型：catalog/invocation split。
- Baseline：06-14 residual。
- 证据：
  - `bootstrap/runtime_gateway.rs:57` 注册 dynamic provider。
  - `runtime_gateway/gateway.rs:90` / `:95` / `:97` 的 `invoke` 查 dynamic providers。
  - `runtime_gateway/gateway.rs:65` / `:73` / `:78` 的 `surface_for_actor` 只遍历 static providers。
  - `workspace_module/mod.rs:238` / `:242` / `:249` 另从 extension projection 枚举 runtime action operation。
- 影响：同一 extension action 在 WorkspaceModule 可发现、RuntimeGateway 可执行，但 RuntimeGateway surface 不可发现。
- 建议：明确 owner：要么 RuntimeGateway surface 动态展开 extension actions，要么声明 discovery 只属于 WorkspaceModule/Extension projection。

### 13. Extension invocation workspace resolver 在 API route 与 workspace module bridge 重复

- Owner：Extension / Workspace Module Runtime Surface / Project Placement。
- 类型：路径冗余 / workspace 权限 owner 不清。
- Baseline：post-fix residual。
- 证据：
  - `routes/extension_runtime.rs:321` / `:332` / `:349` 本地实现 workspace selection。
  - `workspace_module/runtime_bridge.rs:127` / `:140` / `:161` 有同构 helper。
  - `local/handlers/extension.rs:282` / `:286` / `:290` 本机 handler 直接把 `root_ref` 转 `PathBuf`。
- 影响：UI extension panel invoke 与 Agent workspace_module_invoke 可能漂移；default mount fallback 可能选中非本机 workspace mount。
- 建议：抽一个 extension invocation workspace resolver，消费 `RuntimeBackendAnchor + Vfs`，返回 typed local workspace target。

### 14. Workspace module schema validator 比 extension runtime validator 弱

- Owner：Extension / Workspace Module Runtime Surface。
- 类型：duplicated schema authority。
- Baseline：resurfaced。
- 证据：
  - `workspace_module/mod.rs:49` / `:66` 的 schema validator 只检查顶层 type 与 required。
  - `workspace_module/tools.rs:1364` / `:1376` 在 invoke 前使用弱校验。
  - `runtime_gateway/extension_actions.rs:169` / `:361` 使用更完整的 JSON schema subset validator。
  - `runtime_gateway/extension_actions.rs:649` channel input 也使用同一 validator。
- 影响：同一 operation schema 在 workspace module facade 与 runtime gateway 里有两套解释。
- 建议：将 JSON schema subset validator 下沉到共享模块；workspace module 不维护弱 schema authority。

### 15. Runtime action availability 分散在 CapabilityState、WorkspaceModule provider dependency checks、RuntimeGateway support

- Owner：Agent Runtime Session Surface / Extension Runtime。
- 类型：action availability owner drift。
- Baseline：related residual。
- 证据：
  - `runtime_gateway/gateway.rs:65` static surface，`:90` dynamic invoke。
  - `workspace_module/runtime_tool_provider.rs:245` 先查 `CapabilityState`。
  - `workspace_module/runtime_tool_provider.rs:292` / `:311` / `:353` 再查 RuntimeGateway、transport、runtime backend anchor。
  - `workspace_module/runtime_tool_provider.rs:88` / `:304` 缺依赖时仍暴露 diagnostic tool。
- 影响：Agent-visible tool 存在，但实际 runtime action plane 可能不可用。
- 建议：CapabilityState 是 admission input，RuntimeGateway support 是 action availability，AgentRun runtime surface 是 dependency closure；missing dependency 进入 typed diagnostic 而不是普通工具。

### 16. Context file discovery policy 寄生在 mount metadata/provider string，并影响 skill/memory runtime surface

- Owner：VFS & Runtime Tool Surface / Knowledge & Context Surface。
- 类型：职责漂移 / 抽象泄漏。
- Baseline：06-14 residual/new shape。
- 证据：
  - `context/mount_file_discovery.rs:312` / `:325` / `:337` 读 metadata key 或 provider allowlist。
  - `runtime_capability_projection.rs:130` / `:145` / `:150` skill discovery 输出进入 session skill baseline。
  - `runtime_capability_projection.rs:223` / `:239` / `:273` memory discovery 输出进入 memory inventory。
  - `frame_construction/owner_bootstrap.rs:555` / `:567` 写入 `capability_state.skill.skills`。
- 影响：mount 是否可被自动扫描不由 provider/owner typed policy 表达，却会改变模型可见 skill/memory context。
- 建议：引入 typed `RuntimeDiscoveryPolicy`，由 composition owner/provider registry 生成；discovery 只消费 typed policy。

### 17. Mount access、Agent VFS grant、runtime tool capability 是三套并列授权语言

- Owner：VFS & Runtime Tool Surface / Authority & Capability Runtime。
- 类型：概念分叉 / mount ownership 边界不清。
- Baseline：06-14 residual。
- 证据：
  - `mount_project.rs:136` / `:144` / `:148` `AgentVfsAccessGrant` 只裁剪 project VFS mounts。
  - `mount.rs:121` 用 metadata bool 判断 project VFS mount。
  - `owner_bootstrap.rs:363` / `:369` 只在 Project owner 下应用 grants。
  - `tools/factory.rs:46` 起用 `CapabilityState` 控制工具暴露。
  - `mounts.rs:51` / `:62` 又暴露 mount capabilities。
- 影响：未来若 PermissionGrant 要表达 per-mount/path access，没有单一落点。
- 建议：若当前语义正确，重命名为 Project VFS mount grant；若要通用授权，新增 per-mount/path VFS access policy projection。

### 18. Workspace directory fact 写路径分散

- Owner：Project / Workspace / Backend Placement。
- 类型：重复事实源 / API route 过厚。
- Baseline：new。
- 证据：
  - `workspace/backend_sync.rs:38` 已有 `WorkspaceDirectoryFact`。
  - `backend_sync.rs:70` / `:118` 可构造并应用 fact。
  - `routes/backend_access.rs:246` / `:288` manual register 只 upsert inventory。
  - `routes/workspaces.rs:473` / `:616` bind-discovered route 另一套 detect + fact + write。
  - `routes/workspaces.rs:735` / `:886` create/update/hydrate 仍在 route 内推导 workspace shape/binding。
- 影响：同一次 directory detect 可走 inventory-only、candidate/sync、bind-discovered 等多条路径。
- 建议：建立 application-level `WorkspacePlacementService`，统一 detect result -> directory fact -> inventory/binding transaction。

### 19. Desktop Tauri shell 仍拥有 local runtime profile/claim/settings 细节

- Owner：Local Runtime & Relay Surface / Project Placement。
- 类型：06-14 residual / desktop shell 职责漂移。
- Baseline：06-14 residual。
- 证据：
  - `agentdash-local-tauri/src/main.rs:107` / `:124` 定义 `RuntimeStartRequest`、`LocalRuntimeProfile`。
  - `main.rs:244` / `:256` 直接 profile load/save。
  - `main.rs:638` / `:662` / `:752` 执行 ensure claim HTTP flow。
  - `agentdash-local/src/desktop_runner_host.rs:36` 起只接管 runtime lifecycle，未接管 profile/claim。
  - `agentdash-local/src/runner_claim.rs:51` standalone runner 另有 claim client。
- 影响：Desktop 与 headless runner 的 local runtime enrollment/profile 语义继续分叉。
- 建议：profile load/save/normalize、desktop ensure payload/response、claim validation 下沉到 `agentdash-local`；Tauri 只做 adapter。

### 20. 旧 user_preferences 与新 scoped settings 并存

- Owner：Project / Workspace / Backend Placement。
- 类型：重复事实源 / settings 归属漂移。
- Baseline：new。
- 证据：
  - `agentdash-domain/src/settings.rs:22` 起定义 scoped settings。
  - `routes/settings.rs:47` / `:157` 读写 settings repo。
  - `session/launch/preparation.rs:341` / `:366` 从新 settings 读取 `agent.pi.user_preferences`。
  - `domain/backend/repository.rs:25` / `:26` 仍有 `get_preferences/save_preferences`。
  - `postgres/backend_repository.rs:295` / `:320` 仍读写 `user_preferences`。
  - `agent_run/workspace/query.rs:178` / `:189` 与 `routes/lifecycle_agents.rs:1578` / `:1590` 仍读取旧 `hide_system_steer_messages`。
- 影响：用户偏好事实源分裂，BackendRepository 继续承担与 backend placement 无关职责。
- 建议：迁移 `hide_system_steer_messages` 到 scoped settings；移除 BackendRepository preference port，并处理 migration。

### 21. Hook snapshot contribution 在 AgentFrame context summary 之后合并

- Owner：Knowledge & Context Surface / Agent Runtime Session Surface。
- 类型：context injection audit drift / duplicate facts。
- Baseline：06-14 hook/runtime 边界残留的新形态。
- 证据：
  - `frame_construction/assembly.rs:286` / `:296` 先从 context bundle 生成 surface summary。
  - `agent_run/frame/builder.rs:196` / `:209` 写入 `context_slice_json`。
  - `session/launch/planner.rs:134` / `:139` / `:237` 后续才 merge hook snapshot contribution。
  - `session/launch/preparation.rs:151` / `:156` 用 merge 后 bundle 组装模型可见 assignment context。
  - `agent_run/frame/launch_commit.rs:126` / `:131` commit 只覆盖 capability/VFS/MCP，不重写 context slice。
- 影响：模型实际可见 context 与 AgentFrame summary/audit/resume surface 不一致。
- 建议：hook snapshot contribution 进入 frame construction 之前；或 planner merge 后重新生成 context bundle summary 并提交为唯一 launch-ready context fact。

### 22. 内建 VFS skill discovery 未接收 launch identity

- Owner：Knowledge & Context Surface / VFS Surface。
- 类型：identity boundary drift。
- Baseline：VFS-derived knowledge projection residual。
- 证据：
  - `owner_bootstrap.rs:548` / `:551` / `:555` 已把 identity 传入 skill baseline input。
  - `runtime_capability_projection.rs:96` / `:102` 的内建 `load_skills_from_vfs` 没有 identity 参数。
  - `runtime_capability_projection.rs:145` / `:146` dynamic VFS-first provider discovery 会传 identity。
  - `skill/discovery.rs:44` / `:78` / `:109` dynamic scanner API 明确接收 identity。
  - `skill/loader.rs:120` / `:195` / `:295` 内建 loader read/list 传 `None`。
- 影响：同一 active VFS 中内建 workspace skills 与 dynamic provider skills 使用不同 identity 语义。
- 建议：`load_skills_from_vfs`、builtin discovery read/list 接收并传递 `AuthIdentity`。

### 23. MCP runtime binding 要求 backend anchor 的时序早于 anchor 派生

- Owner：Knowledge & Context Surface / Project Placement。
- 类型：construction order drift。
- Baseline：resurfaced。
- 证据：
  - `capability/resolver.rs:308` / `:310` 在 capability resolution 阶段 materialize MCP preset。
  - `owner_bootstrap.rs:534` / `:536` 传 `backend_anchor: None`。
  - `request_assembler.rs:423` / `:425` companion path 也传 None。
  - `mcp_preset/runtime.rs:127` / `:136` required missing backend anchor 报错。
  - `frame_construction/mod.rs:515` / `:524` 在 closed surface 后才派生 runtime backend anchor。
- 影响：需要 runtime backend anchor 的 MCP preset 在初始 frame construction 中可能失败或静默降级。
- 建议：final VFS closure 后先派生 `RuntimeBackendAnchor`，再 materialize MCP presets；或将 binding source 改名为 `VfsDefaultMountBackendId` 并调整顺序。

## P2

### 24. Runtime tool composer 缺少 callable tool name 唯一性 guard

- Owner：VFS & Runtime Tool Surface / Agent Runtime Session Surface。
- 类型：composition invariant 缺失。
- 证据：
  - `runtime_tools/provider.rs:64` / `:66` 直接 extend provider tools。
  - `session/tool_assembly.rs:75` / `:86` 只 dedupe schemas。
  - `agent_loop/tool_call.rs:351` / `:352` 执行时按第一个同名 tool 查找。
  - `agent/tools/registry.rs:29` / `:30` 另一条路径是 HashMap 覆盖。
- 建议：composition root 校验 callable tool name 全局唯一，schema 与 callable surface 使用一致 source/tool_path/name 语义。

### 25. Local relay read loop 仍集中决定跨 domain command scheduling

- Owner：Local Runtime & Relay Surface。
- 类型：横向耦合。
- 证据：
  - `ws_client.rs:373` 用 `should_handle_in_background(&relay_msg)` 决定 spawn。
  - `ws_client.rs:498` / `:501` 只把 shell exec/read/input/terminate 设为 background。
  - prompt、MCP、extension、materialization handler 都可能较重：`prompt.rs:77`，`mcp_relay.rs:27`，`extension.rs:78`，`materialization.rs:67`。
- 建议：handler 返回 `CommandDispatchPlan` / `ExecutionMode`，WebSocket loop 只执行 plan。

### 26. Workspace root validation 在 ToolExecutor 与 ProcessExecutor 重复

- Owner：Local Runtime & Relay Surface。
- 类型：重复本地执行边界。
- 证据：
  - `tool_executor.rs:20` / `:80` 保存并校验 workspace roots。
  - `process_executor.rs:23` / `:38` 重复保存并校验。
  - `process_executor.rs:71` cwd resolve 依赖自身 root validation。
- 建议：抽 `WorkspaceRootGuard`，ToolExecutor、ProcessExecutor、terminal/extension process 共用。

### 27. Relay prompt 仍跨边界传 ACP ContentBlock JSON 并成对转换

- Owner：Local Runtime & Relay Surface。
- 类型：relay wire abstraction leak。
- 证据：
  - `relay_connector.rs:112` 注释说明 ACP `ContentBlock` JSON 可能 degrade。
  - `relay_connector.rs:433` cloud 侧转换 prompt blocks。
  - `local/handlers/prompt.rs:187` local 侧转回 `UserInputBlock`。
  - `prompt.rs:362` unsupported case fallback text。
- 建议：定义 typed relay prompt input payload；ACP conversion 只留在一个 edge。

### 28. RuntimeGateway surface omits dynamic extension actions while invocation accepts them

- Owner：Extension Runtime / RuntimeGateway。
- 类型：catalog/invocation split。
- 说明：与 P1 Issue 12 重叠；若选择 WorkspaceModule 作为 extension action discovery owner，本项降级为明确文档/contract 工作。

## 已收束 / 健康边界

- Lifecycle cancel 通过 `OrchestrationRuntimeEvent::NodeCancelled` reducer。
- Task boot projection 不再从 runtime absence 推断 Failed；Task execution 主 read model 走 `SubjectExecutionView`。
- `/tasks/{id}/execution` 平行 DTO 路径未发现仍存在。
- RuntimeSession runtime-control 不再暴露 mailbox/actions。
- AgentRun direct steer 产品路径已移除，剩余仅测试支撑。
- Runtime tool composer 已拆为 `SessionRuntimeToolComposer` + domain providers；VFS provider 不再承载 workflow/collaboration/workspace-module。
- Local command handling 已从全域 `CommandHandler` 结构上拆成 `LocalCommandRouter + domain handlers`。
- Extension Host raw workspace root override 已拒绝；process/env permission 已细化；input/output schema 已执行校验。
- Permission contract typed gap、active-only list、前端手写 capability catalog 已收束。
- Runner backend identity 已去 project 化；ProjectBackendAccess 是 project->backend 授权事实源。
- Execution placement 走 backend execution lease；未再从 VFS mount 反推 execution backend。
- Shared Library 不再作为 runtime skill fact source；运行期读取 Project SkillAsset。

## 建议后续任务拆分

### 第一批：授权/能力 P0

- 修复 tool-level PermissionGrant 写回 CapabilityState。
- runtime admission 改按 effect_frame/current frame 查询 grant。
- 接通或删除 companion capability grant 旧 payload。
- 建立 production `AgentRunEffectiveCapabilityPort`，让 visible capability 与 admission decision 分离。

### 第二批：AgentRun / RuntimeSession 控制面

- 收敛 launch command/source model。
- 统一 command availability resolver 与 command policy。
- 合并 mailbox steering delivery executor。
- 拆 AgentRuntimeDelegate 为 delegate set。

### 第三批：Orchestrated Work gates

- Routine history 增加 runtime status read model。
- 拆 LifecycleDispatchService 内部 owner。
- 拆 CompanionGate resolver 与 delivery adapters。

### 第四批：Extension / Workspace Module

- 建 renderer-aware extension loadability projection。
- 决定 RuntimeGateway dynamic action discovery owner。
- 抽 extension invocation workspace resolver。
- 共用 JSON schema subset validator。

### 第五批：VFS / Local / Placement / Context

- typed RuntimeDiscoveryPolicy 与 typed mount ownership/purpose。
- Project VFS grant 命名收窄或升级为 per-mount/path policy。
- Desktop profile/claim/settings 下沉 `agentdash-local`。
- WorkspacePlacementService 统一 directory fact transaction。
- settings 迁移：旧 `user_preferences` -> scoped settings。
- Hook snapshot context finalization 与 builtin VFS skill identity。
- MCP runtime binding anchor 时序修正。

## 产物索引

- `module-topology.md`
- `research/01-orchestrated-runtime.md`
- `research/02-extension-authority.md`
- `research/03-vfs-local.md`
- `research/04-placement-context.md`
- `research/05-orchestrated-work-surface.md`
- `research/06-agent-runtime-session-surface.md`
- `research/07-extension-workspace-module-surface.md`
- `research/08-authority-capability-runtime.md`
- `research/09-vfs-runtime-tool-surface.md`
- `research/10-local-runtime-relay-surface.md`
- `research/11-project-workspace-backend-placement.md`
- `research/12-knowledge-context-surface.md`

## 验证说明

本轮是只读架构审查，只写入 Trellis 任务文档和 research 产物；未修改业务代码，未运行全量测试。
