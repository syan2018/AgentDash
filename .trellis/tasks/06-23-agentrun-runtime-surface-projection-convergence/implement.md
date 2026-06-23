# AgentRun runtime surface 投影收束执行计划

## 阶段 0：研究与锁定范围

- [x] 创建任务并写入 PRD。
- [x] 派出 research agent 调研散落路径。
- [x] 将散落路径与冗余链路纳入设计。
- [x] 补充核查 Lifecycle Workflow / AgentProcedure 相关 AgentFrame 写入路径，并纳入收束边界。
- [x] 进入实现前，由用户 review `prd.md`、`design.md`、`implement.md`。

## 并行执行方案

本任务按职责拆成 4 条实现 lane，避免多个 subagent 同时修改同一高风险文件。main session 只做计划更新、集成冲突处理、最终 check / spec update / commit / PR。

| Lane | Owner | 写入范围 | 目标 | 依赖 |
| --- | --- | --- | --- | --- |
| A：Frame surface boundary | trellis-implement A | `crates/agentdash-application/src/agent_run/frame/**`、必要的 `mod.rs` export、相关 tests | 新增 `AgentRunFrameSurfaceService` / command/request 类型和写入白名单测试骨架。 | 无；其它 lane 可先基于现有路径实现，main 集成时接入。 |
| B：Canvas / WorkspaceModule | trellis-implement B | `crates/agentdash-application/src/workspace_module/**`、`crates/agentdash-application/src/canvas/**`、必要的应用测试 | `workspace_module_invoke/present` 不再直接 expose/adopt；改为 typed runtime surface request adapter。 | 依赖 Lane A 类型；若 A 未完成，先放本地 adapter trait/调用点，main 集成到正式 service。 |
| C：Skill / Semantic delta / Frontend | trellis-implement C | `session/capability_projection.rs`、`session/dimension/**`、`session/hub/runtime_context_transition.rs`、`packages/app-web/src/features/session/ui/**` | identity-aware skill projection、provider-aware merge、空 capability key delta 不生成、前端不误报 CAPABILITY DELTA。 | 可独立实现；注意不改 Canvas/Permission 业务链路。 |
| D：Permission / API adoption | trellis-implement D | `crates/agentdash-application/src/permission/**`、`crates/agentdash-api/src/routes/permission_grants.rs`、相关 tests | grant apply/revoke 不直接写完整 `CapabilityState` 并 route 不直接 adopt；改为 update request/service path。 | 依赖 Lane A 类型；若 A 未完成，先实现最小 service hook 并标出 main 需集成点。 |

串行集成顺序：

1. 先合入 Lane A，固定 command boundary 和 public/internal module exports。
2. 合入 Lane C，解决 shared projection/delta 逻辑，降低 Canvas/Permission 变更风险。
3. 合入 Lane B，迁移 Canvas / WorkspaceModule 调用点。
4. 合入 Lane D，迁移 Permission/API route adoption。
5. main session 统一运行冗余链路 `rg` 检查、Rust/前端验证、必要 spec update、commit、PR。

并行约束：

- subagent 必须直接实现，不得再 spawn `trellis-implement` / `trellis-check`。
- subagent 不得 revert 其它 lane 的修改；遇到冲突以 main session 集成为准。
- subagent final 必须列出修改文件、验证命令、未完成/需 main 集成点。
- subagent 只在自己 lane 的写入范围内修改文件；确需跨 lane 修改时先在 final 中说明，不直接扩大范围。

## 阶段 1：测试护栏

- [ ] 增加 capability key empty delta 测试，锁定 added/removed 为空时不生成 section。
- [ ] 增加 provider-aware skill merge 测试，覆盖 URI 型 external integration skill 不因 Canvas/VFS refresh 被删除。
- [ ] 增加 Canvas binding/update 回归测试，确认 external integration skill 在 update 后仍存在。
- [ ] 增加 workspace module invoke 边界测试，确认 invoke 不直接走旧 expose/adopt 旁路。
- [ ] 增加 AgentFrame 写入点分类检查，区分 frame construction / launch commit / runtime surface update。
- [ ] 增加 workflow AgentCall materialization 回归测试，确认 AgentProcedure contract 仍通过 lifecycle node composer 写入 frame construction surface，并同步 RuntimeSessionExecutionAnchor / NodeStarted。

## 阶段 2：统一 frame/surface command boundary 骨架

- [ ] 新增 `AgentRunFrameSurfaceService` facade，作为初始化、definition/contract 投影、accepted launch commit、运行期 surface update 的统一 application command boundary。
- [ ] 新增 `AgentRunFrameSurfaceCommand::{Construct, Update}`，其中 `Construct` 包含 dispatch launch anchor、compose launch surface、commit accepted launch，`Update` 包含 runtime surface mutation request。
- [ ] 新增 `RuntimeSurfaceUpdateRequest` typed enum。
- [ ] 新增 AgentRun runtime surface update service / projector skeleton。
- [ ] 实现 projection context resolver：从当前 AgentRun/AgentFrame/delivery runtime/active turn 解析 identity、VFS、MCP、capability、backend anchor、providers。
- [ ] 将 `adopt_persisted_agent_frame_revision` 标记为内部 primitive，并让统一 service 调用它。
- [ ] 将 `AgentFrameBuilder` 定位为 facade 内部 writer primitive；明确 AgentFrame 写入白名单：frame construction service、lifecycle node composer/dispatch materialization、launch commit、runtime surface update service；其它生产路径不得直接写完整 runtime surface。
- [ ] 保持 `FrameConstructionService` 作为 construction composer，不把 Canvas/Permission runtime mutation 逻辑塞入 construction。

## 阶段 3：Lifecycle / AgentProcedure 写入边界收束

- [ ] 审核 `LifecycleDispatchService::create_initial_frame`、`materialize_workflow_agent_node`、`WorkflowAgentNodeFrameComposer` 和 `composer_lifecycle_node` 的职责边界。
- [ ] 保留 workflow agent/session/anchor materialization 在 lifecycle dispatch 层；将 AgentFrame surface 细节限制在 frame composer / construction service。
- [ ] 确认 `AgentProcedureContract` 的 capability/context/mount/hook 输入通过 construction surface draft 写入，不在 lifecycle/workflow 业务模块内手写完整 `CapabilityState`。
- [ ] 将 AgentProcedure 相关写入纳入静态检查白名单，避免后续 `rg "AgentFrameBuilder"` 时被误当成散落旁路，也避免真实旁路混入。
- [ ] 如发现 lifecycle/workflow 模块直接更新 current AgentFrame runtime surface，迁移为 construction request 或 runtime surface update request。

## 阶段 4：Canvas / WorkspaceModule 迁移

- [ ] 将 Canvas expose/adopt 逻辑迁移到统一 update service。
- [ ] 移除或私有化 `expose_canvas_mount_revision_and_adopt` 旧业务入口。
- [ ] 修改 `workspace_module_invoke(canvas.bind_data)`：只做 Canvas domain mutation，然后提交 `CanvasBindingChanged` request。
- [ ] 修改 `workspace_module_present` Canvas 分支：提交 `CanvasVisibilityRequested` request，等待 projection/adoption 成功后再发 presentation event。
- [ ] 修改 Canvas tool helper：不再以 session id 直接写 runtime surface。

## 阶段 5：Skill projection 收束

- [ ] runtime transition skill discovery 使用 projection context identity，不再写死 `identity: None`。
- [ ] `merge_live_vfs_skill_entries` 改为 provider/source/capability identity merge。
- [ ] 审核 companion selected ProjectAgent skill baseline 的 `identity: None`，迁移到同一 projection context 或记录明确无身份原因。
- [ ] 搜索并消除生产路径中业务手写 `SessionCapabilityProjectionInput` 的散落调用。

## 阶段 6：Permission grant 收束

- [ ] `PermissionGrantService` 不再直接从 current frame 拼 `CapabilityState` 并 `with_capability_state` 写 revision。
- [ ] Permission grant apply/revoke 改为产出 transition/update request，由统一 service replay/write/adopt。
- [ ] API route 删除 direct `adopt_persisted_agent_frame_revision` 调用，改为调用 application service。
- [ ] 保留 Grant 状态成功但 live surface adoption 失败的可见错误语义。

## 阶段 7：Semantic delta 与前端展示

- [ ] `CapabilityKeyDimensionDelta::from_delta` 空 delta 返回 `None`。
- [ ] 后端 runtime context frame 只加入有语义变化的 dimension。
- [ ] 必要时新增/调整 VFS/Skill/WorkspaceModule semantic section。
- [ ] 前端不再把纯 VFS/Skill/WorkspaceModule update 标成 capability key delta。
- [ ] 移除或隐藏 `Capability Keys: no change` 展示。

## 阶段 8：冗余链路删除与防回归

- [ ] `rg "with_capability_state\\("`，确认生产 runtime update 写入只在 frame construction/launch commit/统一 update service。
- [ ] `rg "AgentFrameBuilder"`，确认 Lifecycle Workflow / AgentProcedure 命中只属于 frame construction/materialization 白名单。
- [ ] `rg "agent_frame_repo.*build\\|build(self.frame_repo"`，确认 direct frame create/update 有明确 construction/update owner。
- [ ] `rg "adopt_persisted_agent_frame_revision"`，确认业务模块/API route 不直接调用。
- [ ] `rg "expose_canvas_mount_revision_and_adopt"`，确认旧入口删除或仅统一 service 内部可见。
- [ ] `rg "SessionCapabilityProjectionInput"`，确认生产业务路径不手写 projection input。
- [ ] 补静态或单元测试防止新增旁路。

## 最终质量审阅记录

- [x] Lane A 已落地 `AgentRunFrameSurfaceService`、construction/update command 分类、runtime surface request enum 和 AgentFrame 写入白名单测试。
- [x] Lane B 已将 Canvas / WorkspaceModule 业务调用点迁移为 typed runtime surface request adapter；旧 expose/adopt primitive 只保留在 application 内部 adapter 路径中。
- [x] Lane C 已完成 runtime skill projection identity 传递、provider-aware skill merge、空 capability key delta 后端过滤和前端 runtime surface 标签展示。
- [x] Lane D 已将 Permission grant apply/revoke 迁移到 typed runtime surface update service；API route 不再 direct adopt，active-runtime adoption failure 作为可见错误返回。
- [x] 最终 check 已将 `SessionCapabilityService::adopt_persisted_agent_frame_revision` 收回为 crate 内部 primitive，避免 API crate 重新绕过 application update 边界。
- [x] 已将 AgentRun frame/surface command boundary 的可执行契约同步到 backend session spec。
- [ ] 完整 AgentRun projection context resolver 与 Canvas/Permission adapter 全量接入 `AgentRunFrameSurfaceService` 仍可继续收束；当前实现已经消除业务模块/API route 直接写 frame 或 direct adopt 的主要旁路。

## 验证命令

- [x] `cargo test -p agentdash-application --lib`
- [x] `cargo test -p agentdash-api --lib`
- [ ] `cargo check --workspace`
- [x] 如触及前端：`pnpm --filter app-web test -- ContextFrameCard.test.tsx --run`
- [x] 如触及前端：`pnpm --filter app-web run typecheck`
- [ ] 如触及生成契约：`pnpm run contracts:check`
- [x] `git diff --check`

## 风险文件

- `crates/agentdash-application/src/session/capability_service.rs`
- `crates/agentdash-application/src/session/hub/tool_builder.rs`
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
- `crates/agentdash-application/src/workspace_module/tools.rs`
- `crates/agentdash-application/src/canvas/tools.rs`
- `crates/agentdash-application/src/permission/service.rs`
- `crates/agentdash-api/src/routes/permission_grants.rs`
- `crates/agentdash-application/src/session/capability_projection.rs`
- `crates/agentdash-application/src/session/dimension/capability_key.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/mod.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/composer_lifecycle_node.rs`
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs`
- `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs`
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs`
- `packages/app-web/src/features/session/ui/ContextFrameStream.tsx`
- `packages/app-web/src/features/session/ui/contextFrame/SectionRenderers.tsx`

## 回滚点

- 阶段 2 完成后如服务骨架不合适，可回滚新 service 而不影响旧链路。
- 阶段 3 完成后必须先确认 workflow AgentCall materialization 仍能创建一致的 agent/frame/session/anchor/node state，再进入 Canvas 迁移。
- 阶段 4 迁移 Canvas 后必须先通过 Canvas/workspace module tests，再进入 Permission 迁移。
- 阶段 6 Permission 迁移前保留 grant 状态机测试；若 adoption 语义不清，暂停在设计层补充错误返回 contract。
