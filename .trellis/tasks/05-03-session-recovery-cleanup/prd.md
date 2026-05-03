# Session 拉起 / 恢复大清理

## Goal

把 session 模块的拉起（launch）与恢复（recovery/rehydrate）逻辑从当前
"多条隐式路径并行、字段职责交叉、持久化靠约定不靠类型"的状态，
清理到"类型化状态机、事件驱动投影、职责单一、路径收敛"的形态。

## What I Already Know

本任务基于 2026-05-03 对以下文件的逐行审查：

| 文件 | 行数 | 核心职责 |
|------|------|---------|
| `types.rs` | 278 | SessionMeta / SessionExecutionState / SessionBootstrapState / HookSnapshotReloadTrigger / resolve_session_prompt_lifecycle |
| `prompt_pipeline.rs` | 700 | start_prompt_with_follow_up（550+ 行巨型函数）/ reload_session_hook_runtime |
| `hub/facade.rs` | 650 | 14 个 prompt 启动入口 / recover_interrupted_sessions / 基本 CRUD |
| `hub/mod.rs` | 70 | SessionHub 结构定义 |
| `hub/hook_dispatch.rs` | 280 | emit_session_hook_trigger / ensure_hook_session_runtime / request_hook_auto_resume |
| `hub/cancel.rs` | 125 | 取消路径 |
| `hub_support.rs` | 455 | SessionRuntime / TurnExecution / 各种 builder/parser |
| `continuation.rs` | 742 | build_continuation_system_context / build_restored_session_messages |
| `persistence.rs` | 73 | SessionPersistence trait |
| `persistence_listener.rs` | 72 | sync_executor_session_id |
| `memory_persistence.rs` | 440 | MemorySessionPersistence / merge_session_meta / apply_envelope_projection |
| `session_store.rs` | 131 | 废弃的文件系统 persistence（无外部引用） |
| `launch_intent.rs` | 152 | SessionLaunchIntent 类型 |
| `bootstrap.rs` | 254 | SessionBootstrapPlan / derive_session_context_snapshot |
| `augmenter.rs` | 34 | PromptRequestAugmenter trait |
| `stall_detector.rs` | 69 | Stall 检测 |
| `turn_processor.rs` | 254 | SessionTurnProcessor |

前置重构（`04-30-session-pipeline-architecture-refactor`）已完成 hub.rs 拆分、
ExecutionContext → SessionFrame + TurnFrame、per-turn 字段下沉到 TurnExecution、
Bundle 成为主数据面。本任务在此基础上继续清理残留问题。

---

## Issues

### I-01. `last_execution_status` 裸字符串 → 类型化枚举

**现状**：`SessionMeta.last_execution_status: String`，运行时用
`"idle"/"running"/"completed"/"failed"/"interrupted"` 字面量赋值和匹配。
同时存在 `SessionExecutionState` 枚举和 `TurnTerminalKind` 枚举，但持久化层完全绕开。

**影响路径**：
- `meta_to_execution_state()` — 手动字符串 match
- `apply_envelope_projection()` — 字面量赋值
- `prompt_pipeline.rs` — `"running"` 字面量
- `recover_interrupted_sessions` — `"interrupted"` 字面量
- PostgreSQL / SQLite DDL — `DEFAULT 'idle'`

**目标**：引入 `ExecutionStatus` 枚举（`Idle / Running / Completed / Failed / Interrupted`），
`SessionMeta` 字段改为枚举类型，序列化时自动映射 `snake_case` 字符串。消除所有裸字面量。

---

### I-02. `executor_session_id` 同步路径三重冗余

**现状**：同一个 `executor_session_id` 通过三条独立路径写入 `SessionMeta`：

1. `persistence_listener::sync_executor_session_id` — turn_processor 调用，
   读整行 meta → 改一个字段 → 写整行 meta
2. `apply_envelope_projection` — `append_event` 内联投影（新 BackboneEvent 路径）
3. `apply_compat_info_projection` — 兼容路径从 ACP Meta 提取

路径 1 使用 `parse_executor_session_bound_from_envelope`；
路径 3 使用 `parse_executor_session_bound`（不同函数）。
路径 1 的读-改-写模式可能覆盖 `append_event` 投影产生的 event_seq 等字段。

**目标**：统一到 `append_event` 的事件投影路径。删除 `persistence_listener.rs`，
删除独立的 `sync_executor_session_id`。turn_processor 不再直接写 SessionMeta。

---

### I-03. Turn terminal 事件新旧双重解析器

**现状**：两套并行解析器：
- `parse_turn_terminal_event` — 从 ACP Meta JSON 嵌套解析（`SessionInfoUpdate → agentdash_meta → event`）
- `parse_turn_terminal_event_from_envelope` — 从 `BackboneEnvelope.event` 直接解析

`apply_envelope_projection` 用新路径，`apply_compat_info_projection` 用老路径。
两条路径都在 `append_event` 时触发，同一条事件可能被重复匹配写入。

**目标**：确认老路径是否还有实际事件命中（检查 relay connector 是否仍走 SessionInfoUpdate 包装）。
若无 → 删除 `apply_compat_info_projection` 和 `parse_turn_terminal_event`。
若有 → 在 `apply_envelope_projection` 内统一处理，去掉独立的 compat 函数。

---

### I-04. `save_session_meta` 整行覆盖陷阱

**现状**：SQL 层用 `ON CONFLICT DO UPDATE` + `CASE WHEN excluded.last_event_seq >= sessions.last_event_seq`
保护了 event_seq 相关字段不被旧快照回滚。但 title / executor_config / companion_context /
visible_canvas_mount_ids / executor_session_id 仍然无条件覆盖。

应用层的调用模式普遍是"读整行 → 改一两个字段 → 写整行"：
- `prompt_pipeline` — 改 status/turn_id/executor_config → save
- `persistence_listener` — 改 executor_session_id → save
- `recover_interrupted_sessions` — 改 status → save
- `update_session_meta` — 闭包改任意字段 → save

**目标**：为高频单字段更新提供针对性 SQL method（如 `update_executor_session_id`、
`update_execution_status`），减少整行读-改-写。`save_session_meta` 仅用于创建或批量修改。

---

### I-05. `resolve_session_prompt_lifecycle` 判定条件模糊

**现状**：冷启动恢复的判定条件是：
```rust
!has_live_runtime && last_event_seq > 0 && !has_executor_follow_up
```

`has_executor_follow_up` 依赖 `executor_session_id` 是否非空。
但如果上轮 executor 立即失败没发 `ExecutorSessionBound`，`executor_session_id` 为 None，
即使有历史事件也会误走 Rehydrate。反之，如果曾有 `executor_session_id` 但当前 executor
已不可用，也会误判为 Plain（跳过 Rehydrate）。

**目标**：明确 Rehydrate 判定的语义：
- `has_live_runtime` 查的是 connector 里是否有活跃的 executor session（进程级别）
- `executor_session_id` 表示"executor 侧的 session 标识，可用于 follow_up"
- 分离两个概念：① 是否需要恢复上下文（rehydrate） ② executor 侧是否可 follow_up

---

### I-06. Hook runtime 重建的两条不一致路径

**现状**：`SessionRuntime.hook_session` 由两个函数写入：
- `reload_session_hook_runtime` — 调 `enrich_hook_snapshot_runtime_metadata` 填充
  turn_id / connector_id / executor / permission_policy / working_directory
- `ensure_hook_session_runtime` — 不填充任何运行时元信息

**影响**：通过 `ensure` 路径重建的 hook runtime，其 `snapshot.metadata` 是残缺的，
hook 规则在评估时拿不到 executor / working_directory，可能产出不同的评估结果。

**目标**：`ensure_hook_session_runtime` 也应 enrich metadata。如果调用时没有
足够的运行时信息（executor / working_dir），则从最近一次 SessionMeta 或 SessionProfile 推导。

---

### I-07. `start_prompt_with_follow_up` 550+ 行巨型函数拆分

**现状**：该函数承担至少 14 个职责（详见 Goal 段）。每次修改任一环节
都需要通读全函数。PR review 时也无法单独审视某个阶段。

**目标**：拆分为结构化子步骤：

```
Phase 1: validate_and_acquire_lock()
  → executor_config 解析、running 互斥检查
Phase 2: resolve_session_environment()
  → VFS 三级 fallback、working_dir、mcp_servers、flow_capabilities
Phase 3: prepare_hook_runtime()
  → hook runtime 重建/复用/refresh、injection 合并到 Bundle
Phase 4: determine_lifecycle()
  → resolve_session_prompt_lifecycle、build restored state
Phase 5: discover_capabilities()
  → skills、guidelines、baseline_capabilities
Phase 6: assemble_execution_context()
  → system prompt、tools、ExecutionContext 构造
Phase 7: commit_and_spawn()
  → SessionMeta 更新、title 生成、turn processor 启动、connector stream 适配
```

保持外部 API 签名不变，内部用 builder 或 struct pipeline 传递中间状态。

---

### I-08. `running` 状态三重独立标记

**现状**：
1. `SessionRuntime.running: bool` — 手动设置
2. `SessionMeta.last_execution_status` — 持久化字符串
3. `SessionRuntime.current_turn.is_some()` — 注释说"语义等价 running"但独立维护

清理 running 和 current_turn 是分开做的，部分路径只设其一。
`inspect_execution_states_bulk` 检查 `running` 布尔值，不看 `current_turn`。

**目标**：删除 `SessionRuntime.running` 布尔值，统一用 `current_turn.is_some()` 判定。
所有"设 running = false"的地方改为"current_turn = None"。

---

### I-09. `recover_interrupted_sessions` 双分支逻辑不一致

**现状**：
- 有 `last_turn_id`：写一条 `turn_terminal` envelope → 触发 `append_event` 事件投影更新 meta
- 无 `last_turn_id`：直接改 meta 字段 → `save_session_meta`

前者是事件驱动，后者绕过事件系统。后者在事件流里不留痕迹，前端重放时看不到恢复记录。

**目标**：统一为事件驱动路径。无 `last_turn_id` 时生成一个合成 turn_id
（如 `t_recovery_{timestamp}`），写入带此 turn_id 的 interrupted envelope。

---

### I-10. `SessionBootstrapState` 与 `HookSnapshotReloadTrigger` 隐式耦合

**现状**：
```rust
let is_owner_bootstrap = req.hook_snapshot_reload == HookSnapshotReloadTrigger::Reload;
// ...
if is_owner_bootstrap {
    session_meta.bootstrap_state = SessionBootstrapState::Bootstrapped;
}
```

一个名为 `hook_snapshot_reload` 的 per-prompt 信号，搭便车推进了 bootstrap state machine。
Bootstrap state 的转移逻辑散落在 prompt_pipeline 巨型函数的中段，没有独立入口。

**目标**：在 Phase 1 validate 阶段显式判断 lifecycle 并执行 bootstrap state 转移，
不再依赖 `hook_snapshot_reload` 字段做双重语义。`HookSnapshotReloadTrigger` 回归
纯"hook 层"语义。

---

### I-11. Continuation 双路径设计评估

**现状**：
- `build_continuation_system_context_from_events` → markdown 纯文本注入 system prompt
- `build_restored_session_messages_from_events` → 结构化 `Vec<AgentMessage>` 注入 executor 消息历史

两者内部共享 `build_projected_transcript_from_events`，由 `SessionPromptLifecycle.RehydrateMode`
（`SystemContext` vs `ExecutorState`）决定走哪条。

**评估**：当前设计逻辑合理——两种恢复模式服务不同 executor 能力。
但需要确认 `SystemContext` 路径（不支持 repository restore 的 executor）是否仍有活跃用户。
如果所有 executor 都已支持 `ExecutorState`，可以删除 `SystemContext` 路径。

**目标**：保留架构但审计活跃度。若 `SystemContext` 无活跃 executor 使用则标记 deprecated。

---

### I-12. `facade.rs` 的 `launch_xxx_prompt` 方法过度膨胀

**现状**：facade 暴露 8 个命名 launch 入口 + 6 个内部调度方法 = 14 个 prompt 相关方法。
所有 launch 最终走 `launch_prompt_with_intent → start_prompt`，区别仅在 LaunchIntent 三个字段。

**目标**：保留 `launch_prompt_with_intent` 作为唯一公开入口。
将 `launch_http_prompt` 等改为 `SessionLaunchIntent` 上的 const 工厂方法
（已有 `SessionLaunchIntent::http_prompt()` 等），调用方直接构造 intent 传入。

---

### I-13. 删除废弃的 `session_store.rs`

**现状**：`session_store.rs` 实现了基于文件系统的 `SessionStore`（jsonl + meta.json），
但未实现 `SessionPersistence` trait，且在项目中无任何外部引用。
当前持久化已由 `PostgresSessionRepository` / `SqliteSessionRepository` / `MemorySessionPersistence` 覆盖。

**目标**：删除 `session_store.rs` 及 `mod.rs` 中的 `pub(super) mod session_store` 声明。

---

## Execution Plan（分批）

### Batch 1: 低风险清理（I-08, I-13, I-01, I-12）

这些改动相对独立，不涉及核心流程变更：
1. **I-08** 删除 `running` 布尔值
2. **I-13** 删除废弃 `session_store.rs`
3. **I-01** `last_execution_status` 枚举化
4. **I-12** 收敛 facade launch 入口

### Batch 2: 持久化路径统一（I-02, I-03, I-04, I-09）

这些都围绕"SessionMeta 怎么正确更新"：
1. **I-02** executor_session_id 统一到事件投影
2. **I-03** 删除 compat 双重解析器
3. **I-04** 提供针对性 meta update 方法
4. **I-09** recover 统一为事件驱动

### Batch 3: 架构重构（I-07, I-05, I-06, I-10, I-11）

这些涉及 prompt pipeline 核心流程：
1. **I-07** 拆分巨型函数
2. **I-05** 明确 lifecycle 判定逻辑
3. **I-06** 统一 hook runtime 重建
4. **I-10** 解耦 bootstrap state 转移
5. **I-11** 审计 continuation 路径活跃度

## Non-Goals

- 不改变 SessionPersistence trait 接口签名（除了增加针对性 update 方法）
- 不重构 connector / executor 侧逻辑
- 不改变前端 API wire format
- 不改变 Hook 规则评估引擎本身
