# 数据库表收束与迁移基线压缩设计

## 1. 设计目标

最终 schema 只保留拥有独立业务事实、独立生命周期或必要跨 owner 查询/claim 语义的表。owner
内部状态回到 owner row；无人读取的投影和为未来场景预建的 outbox 不进入数据库。

本任务是未上线项目的首发基线重建，不设计旧 schema 升级、双写或兼容读取。

## 2. 最终表决策

| 当前表 | 决策 | 最终 authority |
| --- | --- | --- |
| `views` | 删除 | 当前无产品调用链 |
| `agent_run_control_effects` | 删除 | 当前无生产读写 |
| `agent_run_terminal_projection_outbox` | 删除 | `agent_run_terminal_projection_change` 已保存同一 change |
| `agent_run_terminal_control_correlation` | 删除 | `projection_change.change` 内的 typed `ControlCorrelated` delta |
| `gate_result_delivery_markers` | 删除 | `lifecycle_gates.delivery` owner document |
| 两张 `agent_run_canvas_*` latest-state 表 | 合并 | `agent_run_canvas_state` |
| `groups` / `group_memberships` | 保留 | 可搜索、可离线查询的 Identity Directory |
| `agent_run_lineages` | 保留 | 跨 AgentRun fork graph canonical record |

预计业务表由 52 张收束为 46 张。

## 3. Gate delivery owner document

`lifecycle_gates` 新增 `delivery jsonb NOT NULL DEFAULT '{}'::jsonb`，由 typed
`GateResultDeliveryState` 编解码。document 保存当前 result attempt、状态、waiter/parent target、
input handoff receipt、claim token、lease 与时间。

所有 mutation 在单事务内：

1. `SELECT delivery FROM lifecycle_gates WHERE id=$1 FOR UPDATE`；
2. typed decode；
3. 应用 register/claim/complete 状态机；
4. 只更新 `delivery` 与 `updated_at`。

当前没有跨 Gate scan 合同，因此不保留 marker 表或 status index。若未来出现恢复 worker，应先形成
明确的跨 owner claim 需求，再决定 expression index 或独立 queue。

## 4. Canvas state

新表 `agent_run_canvas_state` 使用 `(run_id, agent_id, canvas_mount_id)` 作为主键，保存：

- `canvas_id`
- `agent_run_canvas_ref`
- 当前 frame/delivery trace 定位字段
- `runtime_observation jsonb`
- `interaction_snapshot jsonb`
- `created_at` / `updated_at`

两类更新只写各自 document 列，不能用整行替换覆盖另一类状态。按 `canvas_id` 保留查询/级联索引。
旧两表没有需要保留的数据，baseline 直接建立新表。

## 5. Terminal projection

保留三表：

- `agent_run_terminal_projection_head`：revision/sequence CAS owner；
- `agent_run_terminal_projection`：当前终态；
- `agent_run_terminal_projection_change`：有序 replay/change authority。

commit 事务继续原子更新三者，但不再写 outbox 和 correlation 镜像。控制关联从 typed change delta
读取；当前没有独立 outbox consumer，因此不建立待消费记录。

## 6. AgentRun lineage

`agent_run_lineages` 及 `AgentRunForkGraphStore` 保留。删除没有生产调用的通用
`AgentRunLineageRepository` trait、PostgreSQL 实现和 `RepositorySet` 装配，避免同一表拥有两套
持久化入口。

## 7. Baseline 重建

1. 先在代码与目标 schema 中完成表收束。
2. 用空 PostgreSQL 执行最终单一 `0001_init.sql`。
3. migration 目录不再保留 `0002`～`0116`。
4. 使用 `ALLOW_MIGRATION_BASELINE_REWRITE=1 pnpm run migration:guard` 验证本次授权重写。
5. 删除并重建项目 embedded PostgreSQL data directory；不迁移其中的开发态数据。
6. 外部数据库不自动删除；本任务只验证调用方显式提供的临时/测试数据库。

baseline 只包含 schema、约束、索引、序列和必要扩展。seed 与 runtime data 继续走现有启动或
Repository 路径。

## 8. 风险控制

- Gate delivery 状态机先用现有测试锁定，再替换存储。
- Canvas 两个 document 的独立更新必须有“不覆盖另一列”的并发/顺序测试。
- Terminal 删除镜像前以负向搜索确认没有读取者。
- baseline 从空库验证，不以当前开发库成功作为充分证据。
- 本任务允许重建 embedded 数据目录，但不删除用户未明确指定的外部 PostgreSQL 数据库。
