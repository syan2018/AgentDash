# 清理范围分级

## 统计口径

本文件把 `adversarial-review.md` 的 28 条问题按后续实现可执行性分级：

- Quick：边界清楚、主要在 1-2 个 domain 内，适合开一个主持任务并行快速清理。
- Medium：可实现，但需要小设计或跨 2-3 个模块收口；可以纳入同一主持任务，但应作为独立子任务。
- Design：涉及事实源重定、状态机、权限模型、启动链路或数据迁移，需要单独设计任务，不能塞进快速清理。

统计结果：

- Quick：8 项。
- Medium：8 项。
- Design：12 项。

如果要开一个“快速清理”主持任务，建议只纳入 Quick + 少量 Medium，总量控制在 5-7 条并行线内。

## Quick：适合快速并行清理

### Q1. Companion capability grant payload 暂停或删除旧入口

- 来源：Issue 3。
- 范围：`companion/payload_types.rs`、`companion/tools.rs`、session companion request UI。
- 修改性质：删除/禁用旧 `capability_grant_request/result` 入口，或让 human gate 不再接受该 payload。
- 风险：中低。当前平台 broker 已硬失败，UI 也不提交授权结果。
- 建议：纳入快速清理。

### Q2. Workspace module schema validator 复用 extension JSON schema subset validator

- 来源：Issue 14。
- 范围：`agentdash-workspace-module`、`agentdash-application-runtime-gateway` schema helper 位置调整。
- 修改性质：把已有 validator 下沉/共享，删除 workspace module 弱校验。
- 风险：中低。已有更强 validator 和测试基础。
- 建议：纳入快速清理。

### Q3. Extension invocation workspace resolver 去重

- 来源：Issue 13。
- 范围：`routes/extension_runtime.rs`、`workspace_module/runtime_bridge.rs`、可能新增共享 resolver。
- 修改性质：抽单一 resolver，API route 与 workspace module tool 共用。
- 风险：中低。两处逻辑同构，适合小步收束。
- 建议：纳入快速清理。

### Q4. Runtime tool composer 增加 callable tool name 唯一性 guard

- 来源：Issue 24。
- 范围：`SessionRuntimeToolComposer` 或 `session/tool_assembly.rs`。
- 修改性质：composition root 检查 callable tool name 唯一性，失败时给 provider/source 诊断。
- 风险：低。属于 invariant guard。
- 建议：纳入快速清理。

### Q5. Workspace root validation 抽共享 guard

- 来源：Issue 26。
- 范围：`agentdash-local/src/tool_executor.rs`、`process_executor.rs`、terminal/extension process 调用处。
- 修改性质：抽 `WorkspaceRootGuard`，两类 executor 共用。
- 风险：中低。行为保持一致，主要去重。
- 建议：纳入快速清理。

### Q6. Local relay command scheduling 从 ws_client allowlist 下沉为 handler-declared execution mode

- 来源：Issue 25。
- 范围：`agentdash-local/src/ws_client.rs`、`handlers/mod.rs`、domain handlers。
- 修改性质：handler 返回 `ExecutionMode` / `CommandDispatchPlan`，ws loop 不再维护 command enum allowlist。
- 风险：中。要避免改变 terminal/shell 的 ordering。
- 建议：可纳入快速清理，但应单独子任务。

### Q7. Canvas promoted extension loadability 统一 projection

- 来源：Issue 11。
- 范围：canvas promotion、extension runtime projection、workspace module descriptor、frontend extension tab availability。
- 修改性质：renderer-aware loadability，共享给 tab / module descriptor。
- 风险：中。产品语义明确，但跨前后端。
- 建议：可纳入快速清理，但需要 focused tests。

### Q8. Builtin VFS skill discovery 传递 launch identity

- 来源：Issue 22。
- 范围：`skill/loader.rs`、`runtime_capability_projection.rs`、VFS read/list call chain。
- 修改性质：`load_skills_from_vfs` 与 builtin discovery read/list 接收 `AuthIdentity`。
- 风险：中低。dynamic discovery 已有 identity 形态。
- 建议：纳入快速清理。

## Medium：可实现，但建议单独子任务

### M1. PermissionGrant runtime admission 按 effect_frame_id 查询

- 来源：Issue 2。
- 范围：`effective_capability.rs`、runtime session anchor -> current frame resolution、permission repo 查询。
- 修改性质：`list_active_by_run` 改为 current/effect frame 查询。
- 风险：中。P0，但边界清晰。
- 建议：应优先做；可作为快速清理主持任务的最高优先级子任务。

### M2. Tool-level grant 不再写回 CapabilityState

- 来源：Issue 1。
- 范围：AgentRun effective capability、session tool builder、tool invocation/admission path。
- 修改性质：visible capability 与 admission decision 分离。
- 风险：中高。P0，涉及工具暴露和执行准入。
- 建议：优先级最高，但需要和 M1 同一子任务处理，避免半收束。

### M3. Mailbox steering delivery executor 合并

- 来源：Issue 9。
- 范围：`agent_run/mailbox/scheduler.rs`。
- 修改性质：合并 delegate steering 与 scheduler steering 的 receipt/status/error 语义。
- 风险：中。局部但涉及 mailbox terminal semantics。
- 建议：可作为单独子任务。

### M4. Routine execution history 增加 runtime_status read model

- 来源：Issue 4。
- 范围：routine API/domain contract、Lifecycle/Agent runtime projection、frontend history panel。
- 修改性质：保留 dispatch ledger，新增 derived runtime status。
- 风险：中。需要避免把 dispatch ledger 改成 runtime fact。
- 建议：可做，但不和 gate/dispatch service 大拆混在一起。

### M5. Hook snapshot contribution 纳入 final context fact

- 来源：Issue 21。
- 范围：frame construction、launch planner、AgentFrame commit。
- 修改性质：hook snapshot merge 后重新生成或提前进入 context summary。
- 风险：中。涉及 launch pipeline 顺序。
- 建议：单独子任务，避免和 launch command dedup 同时做。

### M6. MCP runtime binding backend anchor 时序修正

- 来源：Issue 23。
- 范围：capability resolver、frame construction VFS closure、MCP preset runtime binding。
- 修改性质：final VFS anchor 先派生，再 materialize MCP preset，或改 binding source 语义。
- 风险：中。需要 focused tests 覆盖 required binding。
- 建议：单独子任务。

### M7. 旧 user_preferences 迁移到 scoped settings

- 来源：Issue 20。
- 范围：settings repo/API、AgentRun workspace query、backend repository、DB migration。
- 修改性质：`hide_system_steer_messages` 等旧偏好迁入 scoped settings，删除 backend preference port。
- 风险：中。需要 migration。
- 建议：可以快速做，但必须单独开子任务处理 migration。

### M8. Typed RuntimeDiscoveryPolicy 替代 discovery metadata/provider allowlist

- 来源：Issue 16。
- 范围：VFS mount/provider metadata、context discovery、skill/memory projection。
- 修改性质：从 raw metadata/provider string 收束到 typed discovery policy。
- 风险：中高。涉及 VFS + context。
- 建议：需要小设计；不建议塞进第一批快速清理。

## Design：需要额外设计，不适合快速清理

### D1. AgentRun visible capability 与 admission decision 的完整生产边界

- 来源：Issue 1、Issue 4、Issue 15。
- 原因：不仅是改查询或删 mutation，还要确定 `AgentRunEffectiveCapabilityPort` 的 production owner、tool schema exposure 与 invocation admission 的调用点。
- 建议：在 M1/M2 快速修最危险路径后，单独做完整设计。

### D2. LifecycleDispatchService 内部 owner 拆分

- 来源：Issue 5。
- 原因：涉及 run/orchestration、agent/frame/session、association、gate、lineage、reducer bridge 的 transaction shape。
- 建议：单独设计任务。

### D3. CompanionGate resolver 与 delivery adapters 拆分

- 来源：Issue 6。
- 原因：涉及 durable gate、parent/child/human delivery、mailbox receipt、session eventing。
- 建议：与 Companion/Routine gate 统一模型一起设计。

### D4. Launch command/source 单一模型

- 来源：Issue 7。
- 原因：涉及 AgentRun -> RuntimeSession -> FrameLaunchEnvelope 的边界，可能影响 backend placement 和 frame construction。
- 建议：单独设计任务。

### D5. Command availability resolver / command policy 统一

- 来源：Issue 8。
- 原因：涉及 workspace shell status、conversation snapshot、command stale guard、frontend command hook。
- 建议：可以在 AgentRun control surface 设计任务中处理。

### D6. AgentRuntimeDelegate 拆 delegate set

- 来源：Issue 10。
- 原因：涉及 agent loop extension points、hook runtime、mailbox turn boundary、provider observer。
- 建议：单独设计任务。

### D7. RuntimeGateway dynamic extension action discovery owner

- 来源：Issue 12、Issue 28。
- 原因：需要决定 extension action catalog 到底属于 RuntimeGateway surface 还是 WorkspaceModule/Extension projection。
- 建议：先做 Q7/Q3/Q2 等局部收束，再设计 catalog owner。

### D8. Runtime action availability 三层 owner 收束

- 来源：Issue 15。
- 原因：CapabilityState、WorkspaceModule dependency checks、RuntimeGateway provider support、AgentRun runtime surface 都参与 availability。
- 建议：与 D1 / D7 联合设计。

### D9. VFS per-mount/path authorization model

- 来源：Issue 17。
- 原因：需要决定 Project VFS grant 是否只是 project mount 裁剪，还是升级为通用 VFS access policy。
- 建议：单独设计，避免和 Q4/Q8 混合。

### D10. WorkspacePlacementService 统一 directory fact transaction

- 来源：Issue 18。
- 原因：涉及 backend inventory、workspace binding、manual register、sync/candidates/create/update 多入口。
- 建议：需要设计 transaction contract；不建议快速改 route。

### D11. Desktop profile/claim/settings 下沉 agentdash-local

- 来源：Issue 19。
- 原因：跨 Tauri shell、frontend local runtime bridge、agentdash-local library、server ensure API。
- 建议：单独设计/实现任务，避免和 local relay handler 清理混合。

### D12. Relay prompt typed payload

- 来源：Issue 27。
- 原因：涉及 cloud connector、local prompt handler、canonical user input blocks、ACP compatibility。
- 建议：单独设计。

## 推荐快速清理任务范围

建议创建一个父任务：`architecture-quick-convergence`，只做清晰局部收束，不做大设计。

第一批可并行 5 条子任务：

1. Authority quick fix
   - 包含 M1、M2 的最小正确修复。
   - 目标：tool-level grant 不扩大 visible capability，runtime admission 按 effect frame。
2. Extension/workspace module consistency
   - 包含 Q2、Q3、Q7。
   - 目标：schema validator、invocation workspace resolver、renderer-aware loadability。
3. VFS/local guard rails
   - 包含 Q4、Q5、Q6、Q8。
   - 目标：tool name guard、workspace root guard、handler-declared scheduling、builtin skill identity。
4. Mailbox steering consistency
   - 包含 M3。
   - 目标：两条 steering path 共享 receipt/status/error semantics。
5. Settings preference convergence
   - 包含 M7。
   - 目标：旧 `user_preferences` 迁入 scoped settings，并处理 migration。

暂不纳入第一批：

- LifecycleDispatchService 拆分。
- CompanionGateControlService 拆分。
- AgentRuntimeDelegate 拆分。
- Launch command 三层模型收束。
- RuntimeGateway dynamic action discovery owner。
- WorkspacePlacementService。
- Desktop profile/claim/settings 下沉。
- Relay prompt typed payload。

这些都需要设计任务先定 owner 和 contract。
