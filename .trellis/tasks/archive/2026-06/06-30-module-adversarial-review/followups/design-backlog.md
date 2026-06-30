# Design 后续讨论清单

## 目标

本文件承接 `cleanup-scope-triage.md` 中的 Design 类问题，用于后续展开讨论和单独设计任务。这里不安排快速实现，也不把这些问题混入 Quick/Medium 收束任务。

这些问题的共同特征是：它们涉及事实源重定、状态机、启动链路、权限模型、跨层 contract 或迁移策略。直接并行实现容易把当前分叉固化成新的局部补丁。

## 设计议题

### D1. AgentRun visible capability 与 admission decision 的完整生产边界

- 来源：`adversarial-review.md` Issue 1、2、15。
- 快速收束状态：
  - tool-level PermissionGrant 已不再写回 visible `CapabilityState`；
  - runtime projection 已从 run-scoped active grant query 改为 frame-scoped active grant query；
  - 当前剩余关键点是 tool invocation execution entry 尚未完整消费 `AgentRunEffectiveCapabilityPort::admit_tool`，`grant_projection_for_runtime_session` 的 admission decision 仍未成为 production tool execution guard。
- 需要回答：
  - `AgentRunEffectiveCapabilityPort` 是否成为唯一 production runtime capability boundary？
  - tool schema exposure 与 tool invocation admission 的调用点分别在哪里？
  - PermissionGrant tool-level grant 是否只在 execution entry 做 admission？
- 依赖：
  - 快速任务 `06-30-architecture-quick-convergence` 已修最危险的 P0 surface 污染和 frame-scope leak。
- 建议后续任务：`agentrun-effective-capability-boundary-design`。

### D2. LifecycleDispatchService 内部 owner 拆分

- 来源：`adversarial-review.md` Issue 5。
- 需要回答：
  - run/orchestration starter、agent/frame/session materializer、subject association、gate/lineage、reducer bridge 的 transaction 边界。
  - facade 是否保留，哪些内部 use case 拥有写入顺序。
- 建议后续任务：`lifecycle-dispatch-owner-split-design`。

### D3. CompanionGate resolver 与 delivery adapters 拆分

- 来源：`adversarial-review.md` Issue 6。
- 需要回答：
  - `LifecycleGate` 如何只表达 durable gate fact？
  - human/parent/child delivery intent 如何从 gate transition 中分离？
  - Companion / Routine / HumanGate 是否共享同一 gate resolution port？
- 建议后续任务：`companion-routine-gate-boundary-design`。

### D4. Launch command/source 单一模型

- 来源：`adversarial-review.md` Issue 7。
- 需要回答：
  - AgentRun、RuntimeSession、FrameLaunchEnvelope 三层中哪一层拥有 launch command domain model？
  - backend placement 输入是否属于 launch planning，而不是 launch command identity？
- 建议后续任务：`runtime-launch-command-model-design`。

### D5. Command availability resolver / command policy 统一

- 来源：`adversarial-review.md` Issue 8。
- 需要回答：
  - `AgentConversationSnapshot` 是否成为唯一 command availability owner？
  - command policy 如何复用同一 resolver，同时保持 stale guard？
  - workspace shell status 需要保留哪些非控制字段？
- 建议后续任务：`agentrun-command-availability-design`。

### D6. AgentRuntimeDelegate 拆 delegate set

- 来源：`adversarial-review.md` Issue 10。
- 需要回答：
  - context transform、tool policy、compaction、turn boundary、provider observer 是否拆成独立 traits？
  - LaunchPlan 如何显式表达 hook runtime 与 mailbox turn boundary 的组合顺序？
- 建议后续任务：`agent-runtime-delegate-set-design`。

### D7. RuntimeGateway dynamic extension action discovery owner

- 来源：`adversarial-review.md` Issue 12、28。
- 快速收束状态：
  - extension loadability、schema validator、workspace resolver 已收束；
  - dynamic action discovery owner 仍需单独定 owner，不应混入 WorkspaceModule quick fix。
- 需要回答：
  - extension action discovery 到底属于 RuntimeGateway surface，还是 WorkspaceModule / Extension projection？
  - dynamic provider 是否需要 context-aware surface projection hook？
- 依赖：
  - 快速任务已完成 extension loadability、schema validator、workspace resolver 收束。
- 建议后续任务：`runtime-gateway-extension-action-catalog-design`。

### D8. Runtime action availability 三层 owner 收束

- 来源：`adversarial-review.md` Issue 15。
- 需要回答：
  - CapabilityState、RuntimeGateway provider support、AgentRun runtime surface、WorkspaceModule dependency diagnostics 如何分工？
  - missing dependency 是 launch readiness failure，还是 typed resource diagnostic？
- 建议与 D1 / D7 联合讨论。

### D9. VFS per-mount/path authorization model

- 来源：`adversarial-review.md` Issue 17。
- 需要回答：
  - 当前 `AgentVfsAccessGrant` 是否只应命名为 Project VFS mount grant？
  - 是否需要通用 per-mount/per-path policy projection？
  - PermissionGrant 如何表达 VFS path-level admission？
- 建议后续任务：`vfs-access-policy-boundary-design`。

### D10. WorkspacePlacementService 统一 directory fact transaction

- 来源：`adversarial-review.md` Issue 18。
- 需要回答：
  - detect result -> inventory -> binding 的 transaction owner。
  - manual register、candidate/sync、bind-discovered、workspace create/update 的统一 use case。
  - Advanced Maintenance 只改 binding 时如何表达意图。
- 建议后续任务：`workspace-placement-service-design`。

### D11. Desktop profile/claim/settings 下沉 agentdash-local

- 来源：`adversarial-review.md` Issue 19。
- 需要回答：
  - desktop profile、desktop settings、desktop access-token ensure 是否由 `agentdash-local` 拥有。
  - Tauri shell 与 standalone runner 的 shared enrollment client 如何抽象。
  - TS local runtime bridge 与 Rust local library 的 DTO 分工。
- 建议后续任务：`desktop-local-runtime-profile-claim-design`。

### D12. Relay prompt typed payload

- 来源：`adversarial-review.md` Issue 27。
- 需要回答：
  - Relay prompt payload 是否应直接使用 canonical user input contract。
  - ACP ContentBlock conversion 应留在哪个 edge。
  - non-text / image / mention / skill block 如何跨 local relay 保真。
- 建议后续任务：`relay-prompt-payload-contract-design`。

## 不进入快速实现的原因

- 它们多数没有唯一局部修改点。
- 多数会影响 API/contract、runtime launch、permission admission、gate continuation 或 local/desktop product shape。
- 需要先定 owner 和 contract，再拆实现任务。
