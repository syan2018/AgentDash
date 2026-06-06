# Orchestration 领域合同设计

## 意图

本任务把父任务研究模型落成第一批生产代码合同。范围刻意收窄：`LifecycleRun` 具备拥有 orchestration 上下文和快照的能力，但现有静态 graph runtime 继续使用 `WorkflowGraphInstance.activity_state`，直到 compiler 与 common runtime 子任务完成 review 并启动。

## Aggregate 边界

`LifecycleRun` 继续作为 lifecycle 级上下文的 owning aggregate。新增字段如下：

| 字段 | 职责 |
| --- | --- |
| `context` | Lifecycle 级 subject、AgentRun refs、Frame refs、权限/预算摘要。 |
| `orchestrations` | 0..N 个内部 orchestration instance，每个 instance 持有 source、plan snapshot、runtime node state、dispatch 摘要和 state exchange snapshot。 |
| `view_projection` | 可选 read projection 占位。本任务不把它作为 command input，也不让当前 UI 消费它。 |

Repository 存储遵循当前项目约定：领域层使用 typed Rust values，PostgreSQL 以 `TEXT` 保存复杂值对象的 JSON 序列化。新列命名保持领域语义，不使用 `_json` 后缀。

## 领域类型

新增 workflow value-object 模块：

```text
crates/agentdash-domain/src/workflow/value_objects/orchestration.rs
```

通过现有 workflow value-object module re-export。`run_state.rs` 保留当前 Activity runtime state 与 `ExecutorRunRef`，不要把所有新类型塞进去；新合同复用 `ExecutorRunRef`，不重复定义执行身份。

最小类型形态：

- `LifecycleContext`
  - `main_agent_run_id: Option<Uuid>`
  - `agent_runs: Vec<AgentRunRef>`
  - `frame_refs: Vec<AgentFrameRef>`
  - `permission_scope: Option<serde_json::Value>`
  - `budget: Option<serde_json::Value>`
- `AgentRunRef`
  - `agent_run_id`
  - `role`
  - `status`
  - `current_frame_id`
  - `project_agent_id`
- `AgentFrameRef`
  - `frame_id`
  - `agent_run_id`
  - `revision`
  - `procedure_id`
  - `graph_instance_id`
  - `activity_key`
- `OrchestrationInstance`
  - `orchestration_id`
  - `role`
  - `source_ref`
  - `status`
  - `plan_snapshot`
  - `activation`
  - `node_tree`
  - `dispatch`
  - `state_snapshot`
  - `journal_cursor`
  - timestamps
- `OrchestrationPlanSnapshot`
  - `plan_id`
  - `plan_version`
  - `source_ref`
  - `nodes`
  - `entry_node_ids`
  - `activation_rules`
  - `limits`
  - `created_at`
- `RuntimeNodeState`
  - `node_id`
  - `node_path`
  - `kind`
  - `status`
  - `attempt`
  - `inputs`
  - `outputs`
  - `executor_run_ref`
  - `children`
  - `phase_path`
  - timestamps
  - error
  - trace refs
  - cache

权限、预算、cache 等最终产品模型尚未定型的字段，可以先用窄 wrapper 或 `serde_json::Value`。本任务的关键是 ownership 和序列化边界，不是一次性定完所有产品策略。

## 持久化

新增 migration：

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS context text DEFAULT '{}'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS orchestrations text DEFAULT '[]'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS view_projection text;
```

除非实现同时定义并维护明确 revision 语义，否则不添加 `orchestration_revision`。本任务不添加 journal 表，也不添加 trace anchor 的 node 坐标列。

更新 `PostgresLifecycleRunRepository` 映射：

- select / insert / update 列表纳入新增列；
- create / update 时序列化新字段；
- row conversion 时解析新字段，并在错误中带上列名；
- constructor 默认初始化为空值。

## 非目标

- 不改 scheduler 读写路径。
- 不实现 common runtime materialization。
- 不实现 `WorkflowGraph -> OrchestrationPlanSnapshot` compiler。
- 不改 generated workflow DTO 或前端 projection。
- 不从 `WorkflowGraphInstance.activity_state` 回填。
- 不引入长期兼容路径或第二套 runtime 事实源。

## 验证

本合同切片只需要聚焦验证：

- domain serde tests 覆盖 plan snapshot、runtime node、journal facts；
- domain aggregate tests 覆盖 0..N orchestration instances；
- infrastructure repository tests 覆盖 lifecycle run 新列 roundtrip；
- `cargo test -p agentdash-domain orchestration`；
- `cargo test -p agentdash-infrastructure workflow_repository`；
- `pnpm run migration:guard`；
- `git diff --check`。
