# Research: scattered-runtime-surface-paths

- Query: AgentRun/runtime surface/capability/skill/VFS/workspace module 链路中，哪些业务路径在零散重建上下文、直接构造 CapabilityState、直接写 AgentFrame capability revision、直接 adopt runtime surface，或漏传 identity/owner/workspace facts。
- Scope: internal
- Date: 2026-06-23

## Executive summary

当前生产代码中，runtime surface 的事实源已经在规范层明确要求收束到 AgentRun / AgentFrame / FrameLaunchEnvelope 的闭包投影，但仍存在几条业务路径直接写入或采用 AgentFrame revision：

- `workspace_module_invoke` 的 `canvas.bind_data` 分支会在普通 operation dispatch 内调用 Canvas mount refresh，进入 `SessionCapabilityService::expose_canvas_mount_revision_and_adopt`，从当前 frame 投影出 `CapabilityState`，修改 VFS/skills/workspace module refs 后写新 AgentFrame revision 并立即 adopt。
- `workspace_module_present` 的 Canvas renderer 分支同样会先 expose/adopt Canvas runtime surface，再发 `workspace_module_presented`。
- Canvas 专用 tool 的 `expose_canvas_to_session` 也是同一路径，属于 Canvas domain 直接触发 runtime surface mutation。
- Permission grant approve/revoke 编译出 `RuntimeCapabilityTransition` 的同时，也直接从 current frame 重建 `CapabilityState`，写新 AgentFrame revision；API route 再直接调用 `adopt_persisted_agent_frame_revision`。
- `SessionCapabilityService::derive_skill_entries_for_active_vfs` 在 runtime transition / Canvas expose 路径中以 `identity: None` 重跑 skill discovery，而 owner bootstrap 会传入 `spec.identity`。这确认了初始构建和运行期重投影的身份上下文不一致。
- live VFS skill merge 使用 `skill.file_path.contains("://")` 判断旧 skill 是否跳过，风险是把 URI 型 external/provider skill 当成 VFS skill 丢弃，而 `SkillEntry` 已有 `provider_key`/`capability_key` 可作为更稳定身份。
- 后端 ContextFrame 仍把 VFS、skill、MCP、tool schema 等 semantic delta 汇入 `kind="capability_state_delta"`；前端按 frame kind 标为 `CAPABILITY DELTA`，并对空 `capability_key_delta` 显示 `no change`。这会继续造成“runtime surface 变化被能力 key 更新卡片承载”的认知混淆。

没有发现 VFS fs/shell tools、MCP discovery、hook runtime 本身直接写 AgentFrame revision；它们大多消费 `ExecutionContext` 或 target-first hook snapshot。MCP tool assembly 是正例：它把 `ExecutionContext.session.identity`、VFS、runtime backend anchor 传入 discovery call context。

## Files found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-23-agentrun-runtime-surface-projection-convergence/prd.md` | 当前任务 PRD，列出已确认症状和验收标准。 |
| `.trellis/spec/backend/session/architecture.md` | Session/RuntimeSession 事实边界，声明 RuntimeSession 不拥有业务归属或 Agent effective surface。 |
| `.trellis/spec/backend/session/session-startup-pipeline.md` | Frame construction / capability projection normalization / pending runtime command 合同。 |
| `.trellis/spec/backend/session/execution-context-frames.md` | `ExecutionContext` 是 connector-facing projection，不是 application 事实源。 |
| `.trellis/spec/backend/capability/architecture.md` | AgentRun effective capability/admission 是 runtime 能力读取唯一入口。 |
| `.trellis/spec/backend/capability/capability-dimension-pipeline.md` | Runtime transition payload 只保存 declarations/effects，不保存完整 `CapabilityState`。 |
| `.trellis/spec/backend/vfs/vfs-access.md` | Canvas session visibility 当前合同要求 create/present 暴露 Canvas mount 并重跑 skill discovery。 |
| `.trellis/spec/backend/permission/architecture.md` | Permission grant 的 surface-changing grant 会写 AgentFrame revision 并 active-runtime adopt。 |
| `.trellis/spec/backend/hooks/execution-hook-runtime.md` | Hook runtime 使用 frame-first target，runtime session 只表达 delivery provenance。 |
| `.trellis/spec/frontend/state-management.md` | AgentRun workspace 前端命令/状态应从后端 snapshot 和 generated DTO 消费。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | Workspace module presentation 与 AgentRun runtime frame resolution 合同。 |
| `crates/agentdash-application/src/workspace_module/tools.rs` | workspace module create/invoke/present agent tools，包含 Canvas exposure 调用。 |
| `crates/agentdash-application/src/canvas/tools.rs` | Canvas tool 暴露 Canvas 到当前 session 的 helper。 |
| `crates/agentdash-application/src/session/capability_service.rs` | Canvas expose 写 frame/adopt、runtime transition skill baseline refresh。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs` | 已持久化 AgentFrame revision adoption 到 active runtime。 |
| `crates/agentdash-application/src/session/hub/runtime_context_transition.rs` | runtime context transition ContextFrame 生成与 semantic delta 聚合。 |
| `crates/agentdash-application/src/session/dimension/capability_key.rs` | capability key delta section 当前无条件生成。 |
| `crates/agentdash-application/src/session/capability_projection.rs` | skill baseline discovery、identity 输入、live VFS skill merge。 |
| `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs` | owner bootstrap skill baseline 传入身份上下文。 |
| `crates/agentdash-application/src/session/assembler.rs` | companion selected project agent 重新 derive skill baseline 时 `identity: None`。 |
| `crates/agentdash-application/src/permission/service.rs` | permission grant apply/revoke 直接重建 `CapabilityState` 并写新 AgentFrame。 |
| `crates/agentdash-api/src/routes/permission_grants.rs` | permission grant revoke route 直接 active-runtime adopt effect frame。 |
| `crates/agentdash-application/src/session/tool_assembly.rs` | MCP/tool surface assembly，从 ExecutionContext 传递 identity/VFS/backend anchor。 |
| `packages/app-web/src/features/session/model/contextFrame.ts` | 前端解析 `capability_key_delta` 等 ContextFrame sections。 |
| `packages/app-web/src/features/session/ui/ContextFrameStream.tsx` | 前端把 `capability_state_delta` 标为 `CAPABILITY DELTA` 并汇总 runtime update。 |
| `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx` | 空 capability key delta 显示 `no change`。 |
| `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` | 前端收到 `capability_state_delta` 后刷新 workspace state/module catalog。 |

## Confirmed scattered paths table

| 文件/行号 | 调用点 | 问题 | 风险级别 | 建议归口 |
| --- | --- | --- | --- | --- |
| `crates/agentdash-application/src/workspace_module/tools.rs:668` | `WorkspaceModuleInvokeTool::refresh_canvas_mount_for_runtime` | `workspace_module_invoke` 内部 helper 通过 session services 直接调用 Canvas expose/adopt。普通 operation dispatch 因此有 AgentFrame/runtime surface 副作用。 | High | AgentRun runtime surface update service：业务只提交 `CanvasBindingChanged` 或 `CanvasMounted`。 |
| `crates/agentdash-application/src/workspace_module/tools.rs:852` | `WorkspaceModuleOperationDispatch::HostCanvas { BindData }` | `canvas.bind_data` 先更新 Canvas domain 数据，再调用 `refresh_canvas_mount_for_runtime`。operation result 与 runtime surface mutation 混在同一工具分支。 | High | Workspace module invoke 只做 domain operation；runtime projection 由 AgentRun 统一入口根据 changed event 触发。 |
| `crates/agentdash-application/src/workspace_module/tools.rs:1036` | `workspace_module_present` Canvas renderer 分支 | present Canvas 前直接调用 `expose_existing_canvas_for_session`，间接进入 Canvas expose/adopt。展示动作和 runtime surface projection 绑定。 | Medium | `workspace_module_present` 可提交 `CanvasPresented/CanvasVisibilityRequested`，由 AgentRun surface projector 决定是否写 frame/adopt。 |
| `crates/agentdash-application/src/canvas/tools.rs:244` | `expose_canvas_to_session` | Canvas domain helper 直接要求 RuntimeSession id，并调用 capability service expose/adopt 后替换 shared runtime VFS。 | Medium | Canvas tool 只表达 Canvas exposure request；AgentRun 入口解析 target frame/session/VFS/shared cache。 |
| `crates/agentdash-application/src/session/capability_service.rs:103` | `expose_canvas_mount_revision_and_adopt` | 从 `session_id` 反查 target frame，读取 current frame，`project_capability_state_from_frame` 得到 `before_state`，手工追加 Canvas mount/visible module ref，写新 frame 并 adopt。该方法本身承担了业务 mutation、projection、persistence、live adoption 四个职责。 | High | 收束为 AgentRunRuntimeSurfaceProjector / AgentRunRuntimeSurfaceUpdateService；该函数降级为内部 adapter 或删除。 |
| `crates/agentdash-application/src/session/capability_service.rs:118` | `project_capability_state_from_frame(&current_frame)` | Canvas expose 以当前 AgentFrame JSON 反序列化出完整 `CapabilityState`，再局部改 VFS/skills。调用方不是 AgentRun projection context，缺少统一 identity/owner/workspace facts。 | High | AgentRun projection context 一次性解析 current frame、identity、workspace、owner、delivery runtime、providers。 |
| `crates/agentdash-application/src/session/capability_service.rs:136` | `AgentFrameBuilder::new(...).with_capability_state(&after_state)` | Canvas expose 非 bootstrap/commit 生产路径直接写 AgentFrame capability/VFS/MCP surfaces。 | High | 只允许统一 surface update 入口调用 frame builder。 |
| `crates/agentdash-application/src/session/capability_service.rs:143` | `next_frame.append_visible_canvas_mount` / `append_visible_workspace_module_ref` | visible canvas/module refs 在写 frame 后又通过 frame mutator 追加，和 `CapabilityState.workspace_module`/VFS surface projection 分离。 | Medium | 作为 runtime surface projection 输出的一部分由统一 projector 生成，不在 Canvas helper 里 append。 |
| `crates/agentdash-application/src/session/capability_service.rs:150` | `adopt_persisted_agent_frame_revision(AgentFrameRuntimeTarget { ... })` | Canvas expose 写完 frame 后立即 active-runtime adopt；业务 helper 掌控 adoption 时机。 | High | AgentRun update service 统一决定 immediate adopt / pending apply。 |
| `crates/agentdash-application/src/session/capability_service.rs:242` | `derive_session_skill_baseline(SessionCapabilityProjectionInput { identity: None, ... })` | runtime transition / Canvas expose 重跑 skill discovery 时丢失 user/group/org identity。external integration skill 可能与 owner bootstrap 不一致。 | High | ProjectionContext 必须携带 AuthIdentity；禁止业务路径手写 `SessionCapabilityProjectionInput`。 |
| `crates/agentdash-application/src/session/capability_projection.rs:268` | `merge_live_vfs_skill_entries` | 用 `skill.file_path.contains("://")` 判断已有 skill 是否来自 VFS 并跳过；URI 型 external/provider skill 会被误判为旧 VFS skill。 | Medium | 按 `provider_key`、source kind、capability key 做 merge policy；不要按 path 字符串形态判断来源。 |
| `crates/agentdash-application/src/session/assembler.rs:465` | companion selected project agent `derive_session_skill_baseline(... identity: None ...)` | 选中 ProjectAgent 的 companion assembly 使用 VFS/MCP runtime context，但 skill baseline 无身份。与 owner bootstrap 的 identity-aware discovery 不一致。 | Medium | 将 companion child frame construction 纳入同一 owner/AgentRun projection context。 |
| `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:551` | owner bootstrap `derive_session_skill_baseline(... identity, ...)` | 这是对照组：初始 owner bootstrap 已把 `spec.identity` 传给 skill discovery，证明 identity-aware 路径存在。 | Reference | 作为统一入口的期望行为。 |
| `crates/agentdash-application/src/permission/service.rs:322` | `PermissionGrantService::apply_grant_effect` | permission grant service 编译 transition 后又读取 current frame、直接构造 next `CapabilityState` 并写 frame。Grant effect 分类和 AgentRun projection 没有成为唯一事实源。 | High | Permission 只输出 `RuntimeSurfaceUpdateRequest::PermissionGrantApplied/Revoked` 或 transition records；AgentRun 统一 replay/write/adopt。 |
| `crates/agentdash-application/src/permission/service.rs:351` | `frame_repo.get_current(anchor_frame.agent_id).unwrap_or(anchor_frame)` | 使用 `effect_frame_id` 找 anchor，再取 agent current frame；未显式从 delivery runtime anchor / AgentRun projection context 解析当前 surface。 | Medium | 从 grant source runtime session / run / agent 解析 current AgentRun frame target。 |
| `crates/agentdash-application/src/permission/service.rs:358` | `project_capability_state_from_frame(&current_frame)` | Permission grant 直接从 frame JSON 还原完整 `CapabilityState` 并局部应用 paths。 | High | 由 capability dimension pipeline replay transition 到 AgentRun current projection。 |
| `crates/agentdash-application/src/permission/service.rs:362` | `AgentFrameBuilder::new(...).with_capability_state(&next_state)` | Permission 非 bootstrap/commit 生产路径直接写 AgentFrame capability revision。 | High | 只允许统一 surface update 入口调用 frame builder。 |
| `crates/agentdash-api/src/routes/permission_grants.rs:279` | `adopt_grant_effect` route helper | API route 在 revoke 后直接调用 session capability adoption，route 层掌控 live runtime adoption。 | High | route 只调用 application service；application/AgentRun service 返回 structured adoption result。 |
| `crates/agentdash-api/src/routes/permission_grants.rs:291` | `session_capability.adopt_persisted_agent_frame_revision(...)` | Permission effect frame adoption 直接从 route 发起，绕过统一 AgentRun command/update boundary。 | High | AgentRun surface update service 内部完成 active runtime sync，并把失败作为 service result。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs:183` | `SessionRuntimeInner::adopt_persisted_agent_frame_revision` | adoption helper 是当前 live runtime sync 中枢，负责校验 frame target、重装 tool surface、更新 active turn cache、同步 hook runtime、发 ContextFrame。它本身可保留为内部 primitive，但目前被 Canvas/Permission 业务路径直接调用。 | Medium | 保留为 AgentRun update service 内部 primitive，不对业务模块/API route 直接暴露。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs:314` | `emit_adopted_runtime_context_transition` | persisted revision adoption 会发 `capability_state_delta`，`key_delta` 是 default，而 `state_delta` 可包含 VFS/skill/MCP 等非 capability key 变化。 | Medium | semantic delta frame kind 拆分：capability key delta、runtime surface delta、skill/VFS/workspace module delta。 |
| `crates/agentdash-application/src/session/dimension/capability_key.rs:19` | `CapabilityKeyDimensionDelta::from_delta` | 无条件返回 `Some`，即使 capability key added/removed 为空，也生成 capability key section。 | Medium | added/removed 均空时返回 `None`；有效 capability 集合如需展示应进入 snapshot/summary section。 |
| `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:414` | runtime context frame 总是 push capability key dimension | 因 `CapabilityKeyDimensionDelta::from_delta` 无条件 Some，VFS/skill-only runtime update 也会携带 `Capability Keys: no change` section。 | Medium | frame builder 只加入有变化的 dimension；非 key surface 改变进入独立 section/frame。 |
| `packages/app-web/src/features/session/ui/ContextFrameStream.tsx:102` | `FRAME_KIND_LABELS.capability_state_delta = "CAPABILITY DELTA"` | 前端以 frame kind 而非 semantic sections 命名整帧；VFS/skill/workspace module-only 变化会被标题化为能力 delta。 | Medium | 根据 sections/semantic frame kind 展示，如 `RUNTIME SURFACE`、`SKILLS`、`VFS`。 |
| `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx:116` | capability key section hint | 空 key delta 显示 `no change`，直接对应 PRD 中“无能力 key 变更”的空展示。 | Low | 后端不生成空 section；前端可兜底隐藏空 capability_key_delta。 |
| `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:457` | `context_frame` event handling | 收到 `capability_state_delta` 就刷新 workspace state/module catalog/hook runtime。这是合理 invalidation，但因为 frame kind 宽泛，会让 semantic delta 与 capability delta 继续耦合。 | Low | 后端拆分 frame kind 后，前端按 semantic event/frame kind 刷新对应投影。 |

## Code patterns

### Pattern A: business tool -> Canvas domain mutation -> frame write/adopt

- `WorkspaceModuleInvokeTool` 的 `canvas.bind_data` 分支在 `crates/agentdash-application/src/workspace_module/tools.rs:852` 进入 `HostCanvas::BindData`。
- 它在 `crates/agentdash-application/src/workspace_module/tools.rs:875` 调用 `bind_canvas_data_for_project` 更新 Canvas domain 数据。
- 随后在 `crates/agentdash-application/src/workspace_module/tools.rs:881` 调用 `self.refresh_canvas_mount_for_runtime(&canvas)`。
- helper 在 `crates/agentdash-application/src/workspace_module/tools.rs:680` 取出 session services，并在 `crates/agentdash-application/src/workspace_module/tools.rs:682` 调用 `expose_canvas_mount_revision_and_adopt`。

风险：operation dispatch 本应只执行 module operation；这里把 Canvas mutation、runtime VFS refresh、skill rediscovery、AgentFrame revision write、active-runtime adoption、ContextFrame emission 绑定成一个不可分的同步 side effect。

### Pattern B: Canvas expose helper owns projection/write/adopt

- `SessionCapabilityService::expose_canvas_mount_revision_and_adopt` 从 `session_id` 反查 target，见 `crates/agentdash-application/src/session/capability_service.rs:108`。
- 它读取 target frame，见 `crates/agentdash-application/src/session/capability_service.rs:112`。
- 它从 frame 还原 `before_state` 并 clone 为 `after_state`，见 `crates/agentdash-application/src/session/capability_service.rs:118`。
- 它从 `after_state.vfs.active` 取 VFS，追加 Canvas mount 并刷新 binding files，见 `crates/agentdash-application/src/session/capability_service.rs:120`、`crates/agentdash-application/src/session/capability_service.rs:126`、`crates/agentdash-application/src/session/capability_service.rs:130`。
- 它重跑 skill baseline，见 `crates/agentdash-application/src/session/capability_service.rs:133`。
- 它用 `AgentFrameBuilder::with_capability_state` 写新 frame，见 `crates/agentdash-application/src/session/capability_service.rs:136`。
- 它再 append visible canvas/workspace module refs，见 `crates/agentdash-application/src/session/capability_service.rs:143`。
- 最后直接 adopt，见 `crates/agentdash-application/src/session/capability_service.rs:150`。

风险：这是最典型的“散装重建上下文 + 直接写 frame revision + 直接 adopt runtime surface”路径。它没有显式携带 identity、owner、workspace facts，而是从 delivery session/frame 局部推导。

### Pattern C: runtime transition skill discovery identity missing

- `SessionCapabilityProjectionInput` 包含 `identity` 字段，见 `crates/agentdash-application/src/session/capability_projection.rs:27`。
- `derive_session_skill_baseline` 会把 VFS 和 identity 合成 `SkillDiscoveryContext`，见 `crates/agentdash-application/src/session/capability_projection.rs:107`。
- owner bootstrap 调用时传入 identity，见 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:551` 和 `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs:554`。
- runtime transition skill refresh 调用时传 `identity: None`，见 `crates/agentdash-application/src/session/capability_service.rs:242` 到 `crates/agentdash-application/src/session/capability_service.rs:245`。
- companion selected ProjectAgent assembly 调用时也传 `identity: None`，见 `crates/agentdash-application/src/session/assembler.rs:465` 到 `crates/agentdash-application/src/session/assembler.rs:468`。

风险：任何依赖 user/group/org context 的 `SkillDiscoveryProvider` 在 owner bootstrap 与 runtime transition 重投影之间会观察不同身份事实。

### Pattern D: live VFS skill merge uses path syntax as source identity

- `merge_live_vfs_skill_entries` 先把 refreshed VFS skills 作为基底，见 `crates/agentdash-application/src/session/capability_projection.rs:268` 到 `crates/agentdash-application/src/session/capability_projection.rs:272`。
- 旧 skill 如果 `file_path.contains("://")` 就被跳过，见 `crates/agentdash-application/src/session/capability_projection.rs:273` 到 `crates/agentdash-application/src/session/capability_projection.rs:275`。
- `SkillEntry` 已有 `capability_key`、`provider_key`、`local_name` 字段，见 `crates/agentdash-spi/src/context/capability.rs:44` 到 `crates/agentdash-spi/src/context/capability.rs:57`。

风险：`file_path` 是展示/定位字段，不是 provider/source identity。URI 型 external integration skill 可能被当成 VFS skill 丢弃，导致 Canvas VFS refresh 后 skill baseline 缩小。

### Pattern E: Permission grant writes surface and route adopts

- `PermissionGrantService::apply_grant_effect` 编译 grant/revoke transition，见 `crates/agentdash-application/src/permission/service.rs:327`。
- 如果存在 surface paths，它读取 effect frame/current frame，见 `crates/agentdash-application/src/permission/service.rs:337` 到 `crates/agentdash-application/src/permission/service.rs:356`。
- 它从 current frame 重建 `CapabilityState`，见 `crates/agentdash-application/src/permission/service.rs:358`。
- 它直接 apply requested paths 并 push toolset effect，见 `crates/agentdash-application/src/permission/service.rs:359` 到 `crates/agentdash-application/src/permission/service.rs:361`。
- 它用 `AgentFrameBuilder::with_capability_state` 写新 revision，见 `crates/agentdash-application/src/permission/service.rs:362` 到 `crates/agentdash-application/src/permission/service.rs:380`。
- API route helper 再调用 active-runtime adoption，见 `crates/agentdash-api/src/routes/permission_grants.rs:279` 到 `crates/agentdash-api/src/routes/permission_grants.rs:295`。

风险：Permission 已经有 `RuntimeCapabilityTransition`，但应用时仍直接拼完整 surface；route 层掌控 adoption，绕过 AgentRun 本体统一 update boundary。

### Pattern F: active runtime adoption primitive is useful but overexposed

- `SessionRuntimeInner::adopt_persisted_agent_frame_revision` 校验 target frame 和 delivery session current frame 一致，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:203` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:246`。
- 它从 adopted frame 投影 state/MCP，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:247`。
- 它通过 `ensure_hook_runtime_for_target` 对齐 hook runtime，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:266` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:275`。
- 它组装 tool surface 并 `connector.update_session_tools`，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:289` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:297`。
- 它更新 active runtime cache，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:299` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:312`。
- 它发 runtime context transition，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:314` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:328`。

结论：这个 primitive 本身不像是错误实现；问题是 Canvas/Permission/API route 直接把它当业务入口。重构时可以保留为 AgentRun runtime surface update service 内部操作。

### Pattern G: semantic delta is wrapped in capability_state_delta

- adoption path 传入 `key_delta: SetDelta::default()`，见 `crates/agentdash-application/src/session/hub/tool_builder.rs:323` 到 `crates/agentdash-application/src/session/hub/tool_builder.rs:325`。
- runtime transition 会计算 full `CapabilityStateDelta`，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:129` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:133`。
- `CapabilityKeyDimensionDelta::from_delta` 无条件返回 `Some`，见 `crates/agentdash-application/src/session/dimension/capability_key.rs:19` 到 `crates/agentdash-application/src/session/dimension/capability_key.rs:29`。
- frame builder 无条件 push capability key dimension，见 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:414` 到 `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:420`。
- 前端把 frame kind `capability_state_delta` 标为 `CAPABILITY DELTA`，见 `packages/app-web/src/features/session/ui/ContextFrameStream.tsx:102` 到 `packages/app-web/src/features/session/ui/ContextFrameStream.tsx:107`。
- 前端空 capability key section 显示 `no change`，见 `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx:116` 到 `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx:120`。

风险：VFS mount、skills、workspace module visibility、MCP/tool schema 的变化虽然有独立 sections，但被包装在 `capability_state_delta` frame 下；空 capability key section 进一步误导用户以为“能力 key delta 发生但无变化”。

### Pattern H: positive examples / no direct mutation found

- `assemble_tool_surface_for_execution_context` 消费 `ExecutionContext`，并把 `context.session.identity`、VFS、backend anchor 放入 MCP call context，见 `crates/agentdash-application/src/session/tool_assembly.rs:55` 到 `crates/agentdash-application/src/session/tool_assembly.rs:68`。
- `WorkspaceModuleRuntimeToolProvider` 通过 `ExecutionContext.session.require_runtime_backend_anchor` 装配 invoke tool，见 `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:268` 到 `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:303`。
- Hook provider 通过 `load_frame_snapshot(AgentFrameHookSnapshotQuery { target, provenance })` 走 frame target，见 `crates/agentdash-application/src/hooks/provider.rs:263` 到 `crates/agentdash-application/src/hooks/provider.rs:289`。

结论：MCP/tool assembly/hook runtime 当前更像 consumer 或 target-first read path，没有发现它们在生产路径直接写 AgentFrame revision。需要关注的是它们被 adoption primitive 刷新时消费的 surface 是否来自统一 AgentRun projection。

## Root cause taxonomy

1. **Runtime surface fact source 分层未彻底落实**

   规范要求 AgentRun / AgentFrame / FrameLaunchEnvelope 是 capability/VFS/MCP/context/identity 的事实闭包，但 Canvas、Permission 等业务路径仍能从 session id 或 grant id 出发自行拼完整 surface。

2. **`CapabilityState` 承载过宽**

   `CapabilityState` 同时包含 tool capability、VFS active surface、skills、workspace module visibility、MCP servers、companion roster 等。业务路径为了改其中一项，必须 clone/serialize/deserialize 整个 state，容易把非能力 key 变化展示成 capability delta。

3. **transition records 与 full projection 并存**

   Permission 已经编译 `RuntimeCapabilityTransition`，但仍直接构造 next `CapabilityState` 并写 frame。Canvas expose 则完全绕开 transition record，直接写 full state。

4. **identity/owner/workspace facts 没有作为 projection context 的必填闭包**

   `SessionCapabilityProjectionInput` 可选地接收 identity；owner bootstrap 传入，runtime transition 不传。调用方越多，漏传越容易。

5. **live adoption primitive 暴露给业务模块**

   `adopt_persisted_agent_frame_revision` 是有价值的同步 primitive，但 API route / Canvas / Permission 能直接调用，导致业务路径自行决定 adoption 时机。

6. **前端以 frame kind 而不是 semantic delta 展示**

   `capability_state_delta` 成为所有 runtime update 的容器，前端直接把容器名当用户语义，放大了后端维度混装的影响。

## Candidate invariant for refactor

建议在 `design.md` 中固化以下不变量：

1. **唯一写入入口**

   生产代码中只有 AgentRun runtime surface update service 可以调用 `AgentFrameBuilder::with_capability_state` / `with_surface_draft` 写 runtime surface revision；bootstrap/accepted commit 是例外且必须被命名为 frame construction / launch commit 路径。

2. **业务只提交 semantic update request**

   Canvas、WorkspaceModule、Permission、MCP、VFS、hook/workflow 等业务模块只能提交类似：

   - `CanvasBindingChanged { canvas_id | canvas_mount_id }`
   - `CanvasVisibilityRequested { canvas_mount_id, reason }`
   - `PermissionGrantApplied { grant_id }`
   - `PermissionGrantRevoked { grant_id }`
   - `McpPresetChanged { preset_id }`
   - `ProjectVfsMountChanged { mount_id }`
   - `WorkspaceModuleVisibilityChanged { module_ref }`
   - `SkillInventoryChanged { provider_key }`

   request 不携带 full `CapabilityState`，不携带 hand-built `SessionCapabilityProjectionInput`。

3. **ProjectionContext 由 AgentRun 当前状态解析**

   统一入口必须从 run/agent/current frame/delivery runtime/session anchor/active turn 中解析：

   - current `AgentFrameRuntimeTarget`
   - current frame surface
   - `AuthIdentity`
   - owner scope / subject association / project id
   - final active VFS and workspace facts
   - runtime backend anchor
   - skill discovery providers and extra dirs
   - current hook runtime target
   - permission/admission projection

4. **transition payload 不保存 full projection**

   runtime delivery command / frame transition fact 保存 `RuntimeCapabilityTransition { declarations, effects }` 或 typed semantic event records；full `CapabilityState` 只在统一 projector replay 后写入 AgentFrame。

5. **semantic delta 与 capability key delta 分离**

   `CapabilityKeyDimensionDelta` 只在 key added/removed 非空时生成；VFS、Skill、WorkspaceModule、MCP、ToolSchema 各自生成独立 semantic section 或独立 frame kind。

6. **Skill merge 用 provider identity**

   Skill baseline merge 以 `provider_key + local_name/capability_key + source kind` 为身份，不以 `file_path` 字符串形态判断来源。

7. **active-runtime adoption 是内部 primitive**

   `adopt_persisted_agent_frame_revision` 只由统一 update service 内部调用。API route 和业务 tool 不直接调用它。

## Main-agent supplement: Lifecycle Workflow / AgentProcedure

用户补充指出 AgentFrame 的主要写入来源还包括 Lifecycle Workflow 的 AgentProcedure 模块。主 Agent 复核后，结论如下：

- `.trellis/spec/backend/workflow/architecture.md` 明确 `AgentProcedure` 是单个 Agent Activity 的 behavior / capability / context / hook / port contract，`AgentFrame` 是 Agent runtime surface revision。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` 中 `materialize_workflow_agent_node` 会创建 `LifecycleAgent`、`RuntimeSession`、`RuntimeSessionExecutionAnchor`，再通过 `WorkflowAgentNodeFrameComposer` 写入 AgentFrame。这是 workflow AgentCall materialization 的合法 construction path。
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs` 中 `create_initial_frame` / `create_plain_initial_frame` 使用 `AgentFrameBuilder::new_launch_anchor` 创建初始 frame。这属于 lifecycle dispatch launch evidence / initial frame construction。
- `crates/agentdash-application/src/agent_run/frame/construction/composer_lifecycle_node.rs` 会从 runtime session anchor 定位 `orchestration_id + node_path + attempt`，读取 plan node，加载或读取 snapshot `AgentProcedureContract`，然后调用 `compose_lifecycle_node_to_frame_with_audit` 和 `compose_pending_frame`。这是 AgentProcedure contract 进入 frame construction surface 的核心路径。
- `crates/agentdash-application/src/agent_run/frame/construction/mod.rs` 的 `FrameConstructionService::compose_pending_frame` 使用 `build_uncommitted` 产出 pending frame，并由 launch commit/adoption 流程后续处理。这说明 Lifecycle node composer 与普通 launch construction 已经共享部分收束点。

该路径与 Canvas / Permission 的冗余链路不同：它不应被删除，而应被明确列入 AgentFrame 写入白名单。后续重构时需要把生产 AgentFrame 写入分成：

1. frame construction / launch commit：owner bootstrap、ProjectAgent、companion、Lifecycle Workflow / AgentProcedure、accepted launch commit。
2. runtime surface update：Canvas、WorkspaceModule、Permission、VFS/MCP/Skill inventory 等运行期变化。

风险是 lifecycle/workflow 代码如果在 composer 外继续扩张 `AgentFrameBuilder` 使用，会形成与 AgentRun runtime surface update service 并列的第二套 surface projector。清理计划应保留 dispatch 层的 agent/session/anchor materialization，但把 capability/VFS/MCP/context surface 细节限制在 frame construction/composer 或统一 runtime update service。

## Suggested implementation phases

1. **Phase 1: guardrail and inventory tests**

   - 增加静态/单元测试锁定非 bootstrap/commit `with_capability_state` 调用点清单。
   - 增加测试确认 `CapabilityKeyDimensionDelta::from_delta` 在 added/removed 为空时不生成 section。
   - 增加测试覆盖 Canvas update 后 external/provider skill 保留，尤其是 URI 型 `file_path` 的 provider skill。

2. **Phase 2: introduce AgentRun runtime surface update service**

   - 新增 update request enum 和 projection context resolver。
   - 首先迁移 Canvas expose：`workspace_module_create/present/invoke` 和 Canvas tool 只提交 request。
   - service 内部仍可复用现有 `adopt_persisted_agent_frame_revision` primitive，但不再对业务模块暴露。

3. **Phase 3: identity-aware skill projection**

   - `derive_skill_entries_for_active_vfs` 改为从 projection context 取 identity。
   - 禁止业务调用方手写 `SessionCapabilityProjectionInput`；保留 owner bootstrap / frame construction 内部调用。
   - `merge_live_vfs_skill_entries` 改为 provider/source identity merge。

4. **Phase 4: permission grant convergence**

   - `PermissionGrantService` 只产出 grant state + semantic surface update request / transition records。
   - approve/revoke route 不直接 adopt；调用统一 AgentRun update service 并返回 structured result。
   - 确认 tool-internal grant 仍只进入 AgentRun admission projection，不写 frame revision。

5. **Phase 5: semantic delta cleanup**

   - 拆分 runtime surface delta frame kind 或至少在 sections 上分离 WorkspaceModule/VFS/Skill/MCP。
   - 前端按 semantic sections/kind 展示，不再把 VFS/skill-only update 标成 `CAPABILITY DELTA`。
   - 移除 misleading `no change` capability key section 展示。

6. **Phase 6: remove old direct entry points**

   - 缩小 `SessionCapabilityService::expose_canvas_mount_revision_and_adopt` 可见性或删除。
   - 缩小 `SessionCapabilityService::adopt_persisted_agent_frame_revision` 对业务模块/API route 的可见性。
   - 用 tests/assertions 防止新增业务路径直接写 AgentFrame capability revision。
   - 将 Lifecycle Workflow / AgentProcedure 相关 `AgentFrameBuilder` 命中纳入白名单检查：只允许 frame construction、lifecycle node composer、workflow AgentCall materialization 和 launch commit；其它 workflow/lifecycle surface 写入必须迁移为 construction request 或 runtime surface update request。

## Related specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/permission/architecture.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/activity-lifecycle.md`
- `.trellis/spec/backend/hooks/execution-hook-runtime.md`
- `.trellis/spec/frontend/state-management.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## External references

No external references were needed. This research is based on repository code, tests, task PRD, and Trellis specs.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, but the user provided `.trellis/tasks/06-23-agentrun-runtime-surface-projection-convergence` explicitly. Output was written only under that task's `research/` directory.
- `workspace_module/tools.rs:1805` and `workspace_module/tools.rs:2036` are `with_capability_state` hits inside tests, not production paths.
- Many `identity: None` hits are tests, default constructors, or synthetic launch fixtures. The production-relevant identity gaps confirmed here are `SessionCapabilityService::derive_skill_entries_for_active_vfs` and `SessionAssembler` companion selected ProjectAgent skill baseline.
- No production VFS fs/shell tool path was found directly writing AgentFrame revision or active runtime surface. VFS mutations appear to operate on provider/overlay/runtime VFS handles; Canvas exposure is the VFS-related exception because it writes the session-visible Canvas mount into AgentFrame surface.
- No direct hook runtime bypass was found. Hook provider uses frame snapshot target APIs; adoption path refreshes hook runtime after persisted frame adoption.
- MCP tool discovery/assembly appears to consume `ExecutionContext` and propagates identity/VFS/backend anchor correctly. The risk is upstream: the active `ExecutionContext` after adoption inherits whatever surface/identity the frame projection wrote.
- `session/hub/facade.rs` sets `FrameLaunchIntent.identity: None` in a test/construction facade path. It is lower-confidence as a production bug than the Canvas/Permission paths and should be reviewed by the main agent before scheduling work.
