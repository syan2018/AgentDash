# Lifecycle 控制面最终收束

## Goal

把 Lifecycle 控制面重构从当前 ~55% 的完成度推进到 100%。本任务是 `06-01-lifecycle-control-plane-residual-hardening`（硬收口）与 `06-01-lifecycle-control-plane-structural-audit`（结构性审计 batch 1-6）的后续收尾；目标模型参照 `06-01-lifecycle-control-plane-concept-alignment` 的讨论结论。

核心链路：

```text
LifecycleRun -> LifecycleAgent -> AgentFrame -> RuntimeSession
SubjectRef -> LifecycleSubjectAssociation -> AgentAssignment -> ActivityAttemptState
```

## Background — 当前基线

### 已完成

- `LifecycleAgent` / `AgentFrame` / `AgentAssignment` / `LifecycleSubjectAssociation` / `LifecycleGate` / `AgentLineage` / `WorkflowGraphInstance` 持久化事实源全部落地。
- `LifecycleDispatchService` 统一入口完成，覆盖 AgentLaunch / SubjectExecution / LifecycleRunStart / InteractionDispatch 四种 intent。
- `WorkflowDefinition` → `AgentProcedure`、`ActivityLifecycleDefinition` → `WorkflowGraph` 命名完成。
- DB 层 `lifecycle_runs.session_id`、`lifecycle_run_links` 表、`session_bindings` 表、`tasks.session_id`、`lifecycle_step_key` 全部 DROP。
- `list_by_session`、`LifecycleRunLink`、`RunLinkSubjectKind`、`RunLinkRole` Rust 代码零引用。
- 前端 `createSession` / `fetchSessions` / `SessionBindingEntity` / `ProjectSessionEntry` 等旧入口已删除。
- `/session/:id` 已降级为 RuntimeSessionTraceView drill-down。
- Permission approve/revoke 产生 AgentFrame revision，route DTO 已收敛到 contracts。
- Routine dispatch_strategy / dispatch_refs 统一完成。
- `AgentFrameHookRuntime` 已建立，Hook rule engine 支持 frame evaluation。
- `AgentFrameSurfaceInput` / `build_lifecycle_activation_surface` 已封装 StepActivation → frame surface。
- `LifecycleRunView` / `SubjectExecutionView` / `ProjectActiveAgentsView` read model builder 已建立。
- 结构性审计 batch 1-6 全部通过（cargo check/test/typecheck 0 errors）。

### 未完成 — 本任务覆盖范围

| 残留面 | 当前引用量 | 核心风险 |
|---|---|---|
| `CompanionSessionContext` on `SessionMeta` | ~32 处 | Companion dispatch/lineage/wait 仍在 session 体系内 |
| `CompanionWaitRegistry` 内存态 | lifecycle_gate_service 已就绪但未接管 |  |
| `HookSessionRuntime` / `SessionHookSnapshot` | ~150 处 | Hook 层仍以 session 为主键；`AgentFrameHookRuntime` 是 wrapper 但覆盖面有限 |
| `Task.agent_binding` | ~82 处 | Task 仍承载执行配置 |
| `SessionConstructionPlan` 旧形态 | ~50 处 | Session 构造仍以 owner-aware bootstrap 为主 |
| `WorkflowContract` 命名 | 全局 | 应为 `AgentProcedureContract` |
| `step_key` 词表残留 | execution log 等 | 应统一为 `activity_key` |
| 前端 session-first 残留 | story session binding、task drawer start/continue | |
| E2E / 集成测试覆盖 | terminal callback, companion gate, routine reuse | |

## Phase Plan

### Phase A: Companion 通道迁移 [P0, 无前置依赖]

**目标**：把 Companion dispatch/lineage/inheritance/wait 从 `SessionMeta.companion_context` + 内存态 `CompanionWaitRegistry` 全部迁入 `LifecycleAgent` + `AgentLineage` + `LifecycleGate` + `AgentFrame` inherited slice。

**具体检查项**：

- [ ] A1: `CompanionSessionContext` 字段拆分
  - `dispatch_id` → `LifecycleGate.correlation_id`
  - `parent_session_id` / `parent_turn_id` → `AgentLineage.source_frame_id` + `metadata_json`
  - `companion_label` / `agent_name` → `LifecycleAgent.agent_role` / `agent_kind`
  - `slice_mode` / `inherited_fragment_labels` / `inherited_constraint_keys` → `AgentFrame.context_slice_json`
  - `adoption_mode` / `request_type` → `LifecycleGate.gate_kind` + `payload`
- [ ] A2: Companion `target=sub` dispatch 通过 `LifecycleDispatchService.open_interaction_gate` 创建 child LifecycleAgent + Gate + Lineage
- [ ] A3: Companion `target=sub` respond / adopt 通过 `LifecycleGateService.resolve_gate` 完成
- [ ] A4: `CompanionWaitRegistry` 内存态等待迁移到 `LifecycleGateService` 的 poll/resolve 模式
- [ ] A5: `SessionMeta.companion_context` 字段删除（或降级为 trace-only provenance）
- [ ] A6: Companion workflow overlay 的 child session/run 创建改为通过 dispatch service 的 `AppendGraph` policy

**验收 Gate**：
```bash
rg "CompanionWaitRegistry" --type rust  # 应仅出现在 lifecycle_gate_service 的过渡注释中
rg "companion_context" crates/agentdash-spi/  # 应删除或标记 deprecated
```

**可并行**: Phase A 与 Phase B 完全独立，可并行执行。

---

### Phase B: Hook Runtime Frame-Native 切换 [P0, 无前置依赖]

**目标**：`HookSessionRuntime` / `SessionHookSnapshot` 全部被 `AgentFrameHookRuntime` 取代，Hook 层以 agent/frame 为主键。

**前置**：结构性审计 batch 1（Hook target gate）已完成封装边界设计。

**具体检查项**：

- [ ] B1: `SessionHookSnapshot` 迁移为 `AgentFrameHookSnapshot`
  - hook provider query 以 `HookControlTarget(run_id, agent_id, frame_id)` 为主键
  - session_id 降级为 `RuntimeAdapterProvenance` 内的 trace ref
- [ ] B2: `HookSessionRuntime` 的 snapshot / trace / pending actions / capabilities / revision 全部迁入 `AgentFrameHookRuntime`
- [ ] B3: `SessionHookService` 的 `ensure_hook_runtime` / `get_hook_runtime` 全部以 `HookControlTarget` 为入口
- [ ] B4: Hook rule evaluation (`rules.rs`、`owner_defaults/`、`fragment_bridge.rs`) 全部以 frame target 为事实源
- [ ] B5: `snapshot_helpers.rs` 中的 session-keyed helper 降级为 trace adapter 或删除
- [ ] B6: Canvas tools 中的 `SessionHookSnapshot` 引用迁移
- [ ] B7: Hook provider tests 全部使用 `AgentFrameHookRuntime` 构造

**验收 Gate**：
```bash
rg "HookSessionRuntime" --type rust  # 应为 0（或仅 deprecated type alias）
rg "SessionHookSnapshot" --type rust  # 应仅出现在 trace adapter / deprecated wrapper
```

**可并行**: Phase B 与 Phase A 完全独立，可并行执行。

---

### Phase C: 业务入口最终收束 [P1, 依赖 Phase A 的 Companion 部分]

**目标**：Task / Story / Routine / ProjectAgent 的所有启动和上下文路径全部且仅通过 `LifecycleDispatchService` 进入控制面。

**具体检查项**：

- [ ] C1: `Task.agent_binding` 迁移
  - 执行相关配置迁到 SubjectExecution request 或 dispatch policy
  - Task 数据可保存用户意图（authoring preference），但运行时只读取 `SubjectRef(kind=Task)` + dispatch policy
- [ ] C2: Task `start_task` / `continue_task` 最终确认只提交 `SubjectExecutionIntent`
  - 前端 task drawer 的 start/continue 调用迁移到 subject execution API
- [ ] C3: `SessionConstructionPlan` 降级为 `RuntimeSessionLaunchPlan`
  - `AgentFrameConstructionPlan` 作为 frame builder 内部细节，不 export
  - `LaunchCommand` 输入变为 Agent/Subject execution intent
  - Session construction 的 context inspection 从 frame 派生
- [ ] C4: Story manual open / freeform 路径确认全部走 `LifecycleDispatchService`
  - Story subject association 端到端验证
- [ ] C5: Routine `Fresh` / `Reuse` / `PerEntity` 确认 agent reuse policy 一致
  - Reuse 不意外创建新 run 的 targeted test
- [ ] C6: ProjectAgent open 确认返回 run/agent/frame/runtime refs
  - 不再返回裸 session_id

**验收 Gate**：
```bash
rg "agent_binding" crates/agentdash-domain/src/task/  # 应为 0 或 deprecated
rg "SessionConstructionPlan" --type rust  # 应重命名或仅出现在 deprecated wrapper
```

**可并行**: Phase C 的 C1/C2 与 C4/C5/C6 之间可以并行；C3 是最大的独立工作块。

---

### Phase D: 命名与 Contract 清理 [P1, 依赖 Phase B/C 基本完成]

**目标**：最终一致的目标词表和 wire contract，不再有混淆命名。

**具体检查项**：

- [ ] D1: `WorkflowContract` → `AgentProcedureContract` / `ProcedureContract`
  - 包含 `WorkflowContract` struct 本身、所有引用、generated TS
- [ ] D2: `step_key` → `activity_key` 词表统一
  - `LifecycleExecutionEntry.step_key` → `activity_key`
  - `EffectiveSessionContract.active_step_key` → `active_activity_key`
  - `GrantScope::WorkflowStep` → `ActivityScope` 或 `FrameScope`
- [ ] D3: Route-local legacy shapes 清理
  - `StoryRunOverviewDto` 确认不含 session-first 字段
  - `TaskResponse` 确认不含 `lifecycle_step_key`
- [ ] D4: Frontend generated types 重新生成并验证
  - `pnpm run contracts:check` 通过
  - Frontend store/component 适配新命名
- [ ] D5: Shared Library import/update 适配新 graph/procedure payload

**验收 Gate**：
```bash
rg "WorkflowContract" --type rust  # 应为 0（除 migration notes）
rg "step_key" crates/agentdash-domain/ crates/agentdash-contracts/  # 应为 0
cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check  # 通过
```

**可并行**: Phase D 内部各项大部分可并行执行。

---

### Phase E: 验证闭环 [P1, 依赖 Phase A-D 全部完成]

**目标**：全栈验证目标模型的正确性和完整性。

**具体检查项**：

- [ ] E1: 后端单元测试补全
  - `LifecycleAgent` / `AgentFrame` / `AgentAssignment` invariant tests
  - Terminal callback → `AgentFrame -> AgentAssignment -> ActivityAttemptState` 端到端
  - Companion durable gate resume/adoption test
  - Story dispatch + subject association test
  - Routine reuse run boundary test
- [ ] E2: 后端集成验证
  - `cargo check --workspace` 0 errors
  - `cargo test --workspace` 全部通过
  - Schema invariant assertions（新表外键、索引覆盖）
- [ ] E3: Contract 验证
  - `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check`
  - `cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check`
- [ ] E4: 前端验证
  - `pnpm --filter app-web run typecheck` 通过
  - Lifecycle store / AgentFrame panel / SubjectExecution panel 基本功能
  - Task drawer subject execution 流程
- [ ] E5: 残留扫描（最终确认）
  ```bash
  rg "list_by_session" --type rust                     # = 0
  rg "lifecycle_step_key" --type rust                  # = 0
  rg "SessionBinding" --type rust --type ts            # = 0
  rg "CompanionWaitRegistry" --type rust               # = 0
  rg "companion_context" crates/agentdash-spi/         # = 0 或 deprecated
  rg "HookSessionRuntime" --type rust                  # = 0 或 deprecated alias
  rg "WorkflowContract" --type rust                    # = 0
  rg "agent_binding" crates/agentdash-domain/src/task/ # = 0
  ```

**验收 Gate**: 所有 E1-E5 检查项全部通过。

---

## Parallelism Map

```text
           ┌─────────────────────┐
           │   Phase A            │
           │ Companion 通道迁移   │─────────┐
           └─────────────────────┘         │
                                            ├──── Phase C ──── Phase D ──── Phase E
           ┌─────────────────────┐         │     (业务入口)     (命名)       (验证)
           │   Phase B            │─────────┘
           │ Hook runtime 切换    │
           └─────────────────────┘

Phase A ∥ Phase B  → 完全并行
Phase C            → 依赖 A（Companion 部分），B 完成后更顺畅
Phase D            → 依赖 B/C 基本完成后执行更安全
Phase E            → 依赖全部完成
```

## Effort Estimate

| Phase | 预估工作量 | 说明 |
|---|---|---|
| A | 中等 | CompanionSessionContext 拆分是主要工作；LifecycleGateService 已就绪 |
| B | 较大 | 150+ 处引用，但封装边界已在审计中确定 |
| C | 中等 | 大部分路径已基本切入，需要最终确认和清理 |
| D | 较小 | 批量重命名 + 重新生成 |
| E | 中等 | 测试编写和全栈验证 |

## Acceptance Criteria

- [ ] Phase A Gate 全部通过
- [ ] Phase B Gate 全部通过
- [ ] Phase C Gate 全部通过
- [ ] Phase D Gate 全部通过
- [ ] Phase E Gate 全部通过
- [ ] `pnpm dev` 可正常启动并运行基本功能

## Out Of Scope

- 不重新讨论 Lifecycle / Workflow / AgentFrame 的概念定义（参照 concept-alignment 任务）。
- 不引入兼容旧 API 的桥接层或双轨机制。
- 不实现 lifecycle branching/fork-join（由 `04-21-workflow-lifecycle-branching-design` 承载）。
- 不实现 companion interaction 完整持久化（由 `05-26-companion-interaction-persistence-model` 承载）。

## Reference Documents

- `.trellis/tasks/06-01-lifecycle-control-plane-concept-alignment/` — 目标模型、语义盘点、谓词体系、重构计划
- `.trellis/tasks/06-01-lifecycle-control-plane-residual-hardening/` — 硬收口记录（已归档）
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/` — 结构性审计 batch 1-6 记录（已归档）
