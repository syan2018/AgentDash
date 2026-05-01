---
task: 04-30-session-pipeline-architecture-refactor
author: claude-agent
started: 2026-04-30
---

# Journal 2 — Session Pipeline 架构级重构（PR 5-7 + DoD）

> journal-1.md 已写到 1892 行，接近 2000 行限制；PR 5 之后的段落迁到 journal-2。
> 前文（PR 1-4 全程、invariant 演进、决策 D1-D6 / E1-E8）见 journal-1.md。

## PR 5 完成 · contribute_* 去重 + 路径统一

2026-04-30

### Commits

- `988d4fd` PR 5a — `refactor(context): workflow_injection 渲染抽共享 helper`
- `d9540f3` PR 5b — `refactor(context): workspace 单源 + SessionPlan 统一外挂`
- `fed69ec` PR 5c — `refactor(context): declared_sources 渲染单点 + slot_orders 集中常量`
- `9cfceb0` PR 5d — `refactor(session): companion bundle fragment 裁剪 + continuation markdown 双包清理 (PR 5d · E8)`

### 关键落位

- 新增 `context/rendering/workflow_injection.rs` — `WorkflowInjectionMode { SummaryOnly, Declarative }` 加两条 per-binding 渲染 helper。`contribute_workflow_binding` / `contribute_lifecycle_context` / `compose_companion_with_workflow` 三处统一走 shared helper；companion+workflow 路径产出经审计总线。
- 新增 `context/rendering/declared_sources.rs` — `source_resolver.rs` + `workspace_sources.rs` 私有 helper 迁出合一；两侧改为 `use context::rendering::declared_sources::*`。
- 新增 `context/slot_orders.rs` — 集中常量；`hooks/fragment_bridge.rs` 的 `HOOK_SLOT_ORDERS` 改 import。
- `context/builtins.rs` 的 `workspace_context_fragment` 扩 `WorkspaceFragmentMode { Full, NoStatus }`；`contribute_core_context` 的 workspace 分支改调 helper。
- SessionPlan 外挂到位：`story/context_builder.rs` / `project/context_builder.rs` `StoryContextBuildInput` / `ProjectContextBuildInput` 砍掉 `session_plan`-相关 4 个字段；各 `compose_*` 在 `session/assembler.rs` 外层显式 push；`compose_lifecycle_node_with_audit` 补上 `build_owner_session_plan_contribution`（以 `SessionPlanPhase::ProjectAgent` 为基座）。
- `slice_companion_bundle`（E8 附带）做 fragment 级裁剪，按 `CompanionSliceMode` 三种模式（`Compact` / `WorkflowOnly` / `ConstraintsOnly`）白名单过滤；`turn_delta` 不裁（hook 层自洽）。4 条单测覆盖全部模式。
- E8 附带 continuation markdown 双包清理：`build_continuation_transcript_fragment` 抽为独立 fragment helper，task continuation 路径不再整体 render → re-wrap。`routes/acp_sessions.rs` 跟进改法；`build_continuation_bundle_from_markdown` 保留作 owner/routine 无 bundle 时兜底。

### 现场决策（5 条）

1. `render_workflow_injection` API 形态：只产出 markdown 文本（enum `WorkflowInjectionMode`），fragment wrapping 留给调用方。三处调用点的 slot/label/order/source 各异，强行抽到 helper 会使签名臃肿。
2. `slot_orders.rs` 为集中定义，`HOOK_SLOT_ORDERS` 引用之；contributor 端散落的 order 数字暂未全部迁移到常量引用（多处散落，风险/收益不匹配，文档标注过渡状态）。
3. `build_continuation_bundle_from_markdown` 保留：transcript 是事件重建的 markdown 不可避免，改为把 transcript 作为 `static_fragment` 独立 fragment upsert。
4. `compose_story_step` 复用 `contribute_story_context` **延后**：会引入 story declared sources 双重解析（base_order 冲突）+ bundle slot order 大幅迁移，snapshot 等价性难以保证。本 PR 只铺垫 `contribute_task_binding`（task-only 字段），完整迁移转给后续 followup。PRD "compose_story_step 调用 contribute_story_context" 完成信号**部分达成**。
5. lifecycle node SessionPlan 基座选 `ProjectAgent`（lifecycle 本质是 project agent 的 step 激活），`workspace_attached: true`。

### 测试

- `cargo build --workspace` 绿（无新 warning）。
- `cargo test --workspace --lib` 全绿；`agentdash-application` lib tests 279 → 289（+4 companion_slice / +6 workflow_injection）。

---

## PR 6 完成 · hub.rs 拆 hub/ 子模块 + event_bridge 清理

2026-04-30

### Commit

- `fccc849` `refactor(session): hub.rs 拆 hub/ 子模块 + event_bridge 清理 (PR 6)`

一个 atomic commit 合并了 7a/7b/7c 建议切分——event_bridge 清理与 `hub/hook_dispatch.rs` / `prompt_pipeline.rs` / `turn_processor.rs` 的 `_tx` 参数清理耦合紧密，强拆为多 commit 需来回 checkout 反而降低 review 效率。

### hub.rs（2801 行）拆分结果

| 文件 | 行数 | 职责 |
|---|---|---|
| `hub/mod.rs` | 69 | `SessionHub` struct + 子模块声明 + `HookTriggerInput` re-export |
| `hub/facade.rs` | 468 | 对外 API：`start` / `cancel` / `subscribe` / `delete` / `ensure` + companion 响应 |
| `hub/factory.rs` | 130 | constructors + `with_*` / `set_*` |
| `hub/tool_builder.rs` | 141 | runtime tool + direct/relay MCP 发现 + `replace_runtime_mcp_servers` |
| `hub/hook_dispatch.rs` | 252 | hook 触发 + snapshot 懒重建 + auto-resume 调度 |
| `hub/cancel.rs` | 123 | cancel + interrupted 事件补发 |
| `hub/compaction.rs` | 131 | `context_compacted` 元数据富化 |
| `hub/tests.rs` | 1724 | 原 hub 17 组单测 |

`hub/mod.rs` (69) + `hub/facade.rs` (468) 两侧入口均 ≤ 500 行 ✅（I7 达标）。

### event_bridge 清理

- `session/event_bridge.rs` (99 行) 整体移除。
- `emit_session_hook_trigger` 的 `_tx: &broadcast::Sender` 占位参数删除；三处调用点（`facade.emit_capability_changed_hook` / `prompt_pipeline` `SessionStart` / `turn_processor` `SessionTerminal`）同步瘦身。

### 现场决策（3 条）

1. **不新建 `SessionHubFactory` 类型**：AppState / local main / companion 三处构造 hub 都走 `new_with_hooks_and_persistence(...).with_*()` 链式 API；引入 factory 包装层会带来 3 个 crate 的调用点迁移成本。工厂职责（`base_system_prompt` / `user_preferences` / `runtime_tool_provider` / `mcp_relay_provider` 注入）已集中到 `hub/factory.rs`，语义层面达成 PRD 目标。
2. **`build_tools_for_execution_context` 保留 `&ExecutionContext`**：PRD 建议签名改为 `(session, mcp)`，但 `runtime_tool_provider.build_tools(context)` trait 层吃整个 `ExecutionContext`；改 trait 超出 PR 6 范围。
3. **`companion_wait` 已独立为 `session/companion_wait.rs`**：hub 只持 `CompanionWaitRegistry` 字段。PRD "归位" 目标天然满足。

### 测试

- `cargo build --workspace` 绿；`cargo test --workspace --lib` 全绿（含 hub 17 组单测）。

---

## PR 7 完成 · turn_processor 净化 + SessionRuntime per-turn 字段下沉

2026-04-30

### Commits

- `273194a` PR 7a — `refactor(session): ActiveSessionExecutionState → TurnExecution + SessionRuntime.current_turn`
- `1a38026` PR 7b — `refactor(session): turn_processor 副作用外移 + persistence_listener 建立`
- `dcca0e7` PR 7c — `refactor(session): auto-resume 限流落 hub_dispatch + hook_session 单向读取`
- `ed63a4a` PR 7d — `refactor(session): build_companion_human_response_notification 归位 companion/ (PR 7d · E8 连带)`

### 关键落位

- `ActiveSessionExecutionState` → `TurnExecution`；吸收原散落在 `SessionRuntime` 的 per-turn 字段：`processor_tx` / `hook_auto_resume_count` / `cancel_requested` / `current_turn_id`。`SessionRuntime.current_turn: Option<TurnExecution>` 字段就位。
- `turn_processor.rs` 净化：
  - 不再直接写 `SessionMeta.executor_session_id`；抽到新建 `session/persistence_listener.rs` 集中处理。
  - `SessionRuntime.running` / `processor_tx` 清理通过 `TurnEvent::Terminal` 由 hub 侧接管。
  - `hook_auto_resume_count` 递增从 processor 移除；processor 只发 `AutoResumeRequested` 信号，**限流逻辑集中到 `hub/hook_dispatch.rs::request_hook_auto_resume`**（原子 CAS 上界）。
- `prompt_pipeline.rs`：`load_session_hook_runtime` → `reload_session_hook_runtime`；尾部不再回写 `hook_session`（`SessionRuntime.hook_session` 单一权威，prompt_pipeline 只读或唤 `ensure`）。
- `build_companion_human_response_notification` 从 `session/continuation.rs` 挪到新建 `companion/notifications.rs`；`continuation.rs` 收窄 8 个类型 imports。

### 现场决策（5 条）

1. auto-resume 测试放在 `session/hub/tests.rs` 末尾，与 `schedule_hook_auto_resume_routes_through_augmenter` 相邻：两条 fail-lock 测试共同锁定 auto-resume 契约（augmenter 必过 + 上限生效）。
2. auto-resume 限流测试用 `build_session_runtime` 手工注入 `SessionRuntime`：`create_session` 只建 meta，必须手工注入 runtime 才能测限流；这条"测试构造"路径同时揭示 runtime vs meta 的生命周期分离（与 I5 相关）。
3. `build_companion_human_response_notification` 选 `companion/notifications.rs` 新文件而非塞 `tools.rs`：`tools.rs` 已 2700 行；notification 不是 tool adapter，独立文件更符合"按事件方向分层"。可见性从 `pub(super)` 放开为 `pub`。
4. `tools.rs` 的 `build_hook_action_resolved_notification` 未一并搬：本 PR 只做明确要求的 `build_companion_human_response_notification`，保持克制。后续 followup 可考虑。
5. `SessionMeta.bootstrap_state` 字段：E7 连带审视，grep 显示仍有持久化路径引用，本 PR 不动；待后续独立评估。

### 测试

- `cargo check -p agentdash-application --lib` ✅
- `cargo test -p agentdash-application --lib session::` ✅ (73/73)
- `cargo test --workspace --lib` ✅（application 291 / spi 37 / agent-types 53 / domain 70 等）

---

## DoD 完成 · 三份 backend spec

2026-04-30

### Specs

| 文件 | 标题 | 行数 | 核心内容 |
|---|---|---|---|
| `.trellis/spec/backend/session/session-startup-pipeline.md` | Session Startup Pipeline | 312 | 5 条正交轴（Who/Where/What/How/Trigger）× 权威字段归属；6 入口统一节拍表；`SessionAssemblyBuilder` first-class 方法契约；`finalize_request` 13 字段对称合并规则；`HookSnapshotReloadTrigger` 语义；I3 / I10 |
| `.trellis/spec/backend/session/execution-context-frames.md` | Execution Context Frames | 322 | `ExecutionSessionFrame` + `ExecutionTurnFrame` 字段所有权/生命周期表；三 connector 消费矩阵；`assembled_system_prompt` 过渡地位与下线路线；`SessionRuntime` vs `TurnExecution` 分离；PiAgent 按 `bundle_id` 热更事件级流程 |
| `.trellis/spec/backend/session/bundle-main-datasource.md` | Session Context Bundle 作为主数据面 | 400 | 双字段（`bootstrap_fragments` / `turn_delta`）语义；`RUNTIME_AGENT_CONTEXT_SLOTS` 契约；Hook 三类语义物理分离；`HOOK_USER_MESSAGE_SKIP_SLOTS` + `session-capabilities://` 废除；Audit Bus × Inspector × `turn_delta` |

`.trellis/spec/backend/index.md` 更新：在 `session/` 小节表格追加 3 行；末尾时间戳刷到 2026-04-30。

---

## Invariant 终态（7 PR 完成后）

| Inv | 状态 | 验证 |
|---|---|---|
| I1 Bundle 作为主数据面 | 🟡 PiAgent 读 Bundle 并按 `bundle_id` 热更；`assembled_system_prompt` 仍作过渡（Relay / vibe_kanban 消化时机 Out of Scope） | PR 3 / spec `execution-context-frames` |
| I2 ExecutionContext 分层 | ✅ `SessionFrame` + `TurnFrame` 就位；三 connector 编译绿 | PR 2 |
| I3 入口单一节拍 | ✅ 6 条入口统一走 `SessionAssemblyBuilder` | PR 1 |
| I4 Hook 三语义分离 | ✅ SKIP_SLOTS / session-caps 零命中；三轴语义独立 | PR 4 |
| I5 SessionRuntime 纯 session 级 | ✅ per-turn 字段下沉到 `TurnExecution`；`current_turn: Option<TurnExecution>` 就位 | PR 7 |
| I6 turn_processor 职责单一 | ✅ 不写 SessionMeta / 不改 running / 不递增 auto_resume | PR 7 |
| I7 hub.rs ≤ 500 行 | ✅ mod.rs 69 + facade.rs 468 | PR 6 |
| I8 contribute_* 单源 | ✅ workflow_injection / workspace / declared_sources 单源；SessionPlan 外挂 | PR 5 |
| I9 slot order 集中 | 🟡 `slot_orders.rs` 就位；`HOOK_SLOT_ORDERS` 引用；contributor 端散落 order 值未全迁移（技术债标注） | PR 5 |
| I10 Routine identity 非 None | ✅ `AuthIdentity::system_routine` | PR 1 |

### 已识别未落地项（followup）

1. **I9 contributor 端 order 常量化**：散落 order 数字未全迁到 `slot_orders.rs` 引用。多处散落、风险/收益不匹配；留作技术债。
2. **`compose_story_step` 完整复用 `contribute_story_context`**：PR 5d 只铺垫 `contribute_task_binding`，完整迁移延后（story declared sources 双重解析风险）。
3. **`companion/tools.rs::build_hook_action_resolved_notification` 归位 `notifications.rs`**：PR 7d 克制，后续可跟进。
4. **`SessionMeta.bootstrap_state` 去留**：E7 连带审视未完成；待后续独立评估。
5. **I1 Bundle α 化**（删除 `assembled_system_prompt`）：Out of Scope 声明的未来工作，需 Relay 协议扩展或本地渲染方案先拍板。

## 分支状态

`refactor/session-pipeline` 领先 main **31 个 commit**（PR 1 的 8 + PR 2 的 2 + PR 3 的 2 + PR 4 的 5 + PR 5 的 4 + PR 6 的 1 + PR 7 的 4 + journal & DoD）。

`cargo test --workspace --lib` 全绿；`cargo build --workspace` 无新 warning。PRD 的所有 acceptance criteria 除部分 followup（I9 contributor 端常量化 / `compose_story_step` 完整迁移）外全部达成。

下一步建议（由 user 裁决）：
- 开 PR 合入 `main`（或按子 PR 粒度分批合）；
- 或先跑 E2E 场景（HTTP prompt / task start / workflow orchestrator / companion dispatch / routine tick / cancel × 3 / compaction）作 acceptance 最后一道闸门。
