# 数据库规范

PostgreSQL + SQLx承载云端业务库与Managed Runtime durable facts。Local Integration Host的执行状态按process incarnation重建，避免把Dashboard schema生命周期引入Desktop与Standalone Runner启动链。

## 基础规则

| 主题 | 规则 |
| --- | --- |
| 错误 | infrastructure 错误映射为 `DomainError`；PostgreSQL repository 使用 `persistence::postgres::db_err` / `sql_err_for`。 |
| 命名 | 数据库列名和 JSON 字段使用 `snake_case`。 |
| 文档列 | 新增结构化文档列使用 `jsonb`，列名表达业务语义，不追加 `_json` / `_jsonb`。 |
| Typed mapping | Domain / repository 边界使用 typed value object；repository 使用 `sqlx::types::Json<T>` 或共享 codec。 |
| JSON codec | PostgreSQL repository 在 SQLx 边界绑定 `jsonb` 为 `serde_json::Value` / `sqlx::types::Json<T>`，再映射到领域类型；错误上下文包含 `table.column`。 |
| 历史 TEXT JSON | 既有 `TEXT` JSON 是历史 schema 事实；批量转换只放进明确数据库整理任务。 |
| Scalar 字段 | 高频过滤、排序、权限判断、claim/lease 状态使用 PostgreSQL scalar 列。 |
| 时间 | 时间字段使用 PostgreSQL timestamp，repository 直接 bind/read `chrono::DateTime<Utc>`。 |
| Repository | 实现模式见 [repository-pattern.md](./repository-pattern.md)。 |

## 事务边界

| 场景 | 边界 |
| --- | --- |
| 单一聚合 | 对应 Repository 管理事务。 |
| 跨聚合 | Application command / Unit of Work 编排多个 explicit ports。 |
| Story projection + LifecycleRun Task facts | Application 编排 `StoryRepository` 与 LifecycleRun task port。 |
| Owner document mutation | Repository 在事务内锁 owner row，typed decode，应用 domain mutation，只写目标 document column 和 `updated_at`。 |

---

## Scenario: Agent Runtime Fact Storage Boundary

### 1. Scope / Trigger

新增 Agent / Lifecycle / Project runtime fact、通信关系、inbox/outbox、capability projection source、adapter binding、delivery planning 或 scheduler 状态。

### 2. Signatures

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL;
```

```rust
pub struct LifecycleRun {
    pub channel_registry: ChannelRegistryDocument,
}

pub trait ChannelOwnerStore {
    async fn load_registry(
        &self,
        owner: ChannelOwner,
    ) -> DomainResult<ChannelRegistryDocument>;

    async fn mutate_registry(
        &self,
        owner: ChannelOwner,
        mutation: ChannelRegistryMutation,
    ) -> DomainResult<ChannelRegistryDocument>;
}
```

```sql
SELECT channel_registry
FROM lifecycle_runs
WHERE id = $1
FOR UPDATE;

UPDATE lifecycle_runs
SET channel_registry = $2,
    updated_at = NOW()
WHERE id = $1;
```

### 3. Contracts

- Owner-local、随 owner 生灭、owner-scoped 查询的事实保存为 typed owner document。
- 独立 store 只承担明确执行语义：跨 owner scan、多 worker claim、独立 retention/audit、跨 owner 索引、数据库唯一约束保护的跨聚合不变量。
- Capability projection 从 owner facts 派生，可重建。
- Owner document 写入走语义 mutation port；business layer 不传 table / column 字符串。
- Broad aggregate update 保留独立 document column 当前值。`LifecycleRunRepository::update` 更新 orchestration/task/status；`channel_registry` 由 `ChannelOwnerStore::mutate_registry` 写入。

### 4. Validation & Error Matrix

| 条件 | 合同 |
| --- | --- |
| fact 随单个 owner 创建、恢复、删除 | owner document |
| fact 只被 owner-scoped API / service 查询 | owner document |
| fact 是 capability / prompt / UI 可见面的输入 | owner document -> projector |
| fact 需要跨 owner 状态扫描、claim、retention、唯一约束或查询索引 | 独立 store candidate |
| `jsonb` document 反序列化失败 | mapped `DomainError` 带 `table.column` context |
| owner row 不存在 | NotFound |
| mutation 违反 typed invariant | validation / conflict error，document 不写回 |
| broad aggregate update 与 document mutation 交错 | aggregate update 保留独立 document column |
| 需要 document 内字段过滤或排序 | `jsonb` operator / expression index，或提升为 scalar 列 |

### 5. Cases

- Runtime channel registry: `LifecycleRun.channel_registry` + `ChannelOwnerStore::mutate_registry`。
- Project 公共 Channel: 当前只定义 owner store port；物理承载由 Project Assets 决定。
- 跨 run scan / claim / recovery 队列: 独立表。
- Boundary mismatch: load `LifecycleRun` -> 改 `run.channel_registry` -> `LifecycleRunRepository::update(run)`。

### 6. Tests Required

- Owner document repository roundtrip: default、非空、shape mismatch context。
- Owner document mutation: 连续 mutation 不丢失、只更新目标 document column、broad aggregate update 保留独立 document column。
- Migration: `pnpm run migration:guard` + 干净数据库初始化。
- Application service: owner document 更新通过语义 mutation port。
- Projection: 从 owner document 派生，不反写事实源。
- 独立表: 覆盖其声明的 scan / claim / retention / unique / query 语义。

### 7. Boundary / Canonical

```text
runtime relation scoped to one LifecycleRun
  -> LifecycleRun document
  -> typed owner document mutation
```

```text
channel registry update
  -> ChannelOwnerStore::mutate_registry(owner, mutation)
  -> SELECT channel_registry FOR UPDATE
  -> apply ChannelRegistryMutation
  -> UPDATE channel_registry, updated_at
```

```text
global worker queue
  -> item store with status, claim_token, claim_expires_at
  -> workers scan by status/lease/order across owners
```

---

## Scenario: Schema Source Of Truth

### 1. Scope / Trigger

PostgreSQL schema、embedded PostgreSQL schema、migration runner、repository readiness、baseline squash / reset / merge。

### 2. Signatures

```text
crates/agentdash-infrastructure/migrations/
agentdash-server migrate
agentdash-server serve
pnpm run migration:guard
```

### 3. Contracts

- `crates/agentdash-infrastructure/migrations/` 是 PostgreSQL schema 事实源。
- 普通 schema 变更新增 migration 文件。
- 已提交 migration 是历史事实；baseline squash / reset / merge 任务必须在任务文档写明授权范围、重建要求和验证命令。
- Repository 只观察已迁移 schema。API bootstrap 执行 readiness check，不创建表、补列、建索引或迁移数据。
- Embedded PostgreSQL 复用同一套 migrations 与 readiness check。
- 初始化 migration 只放 schema、约束、索引、序列和必要扩展；seed / runtime data 由启动期 seed、API use case 或 runtime repository 写入。
- 替换 baseline 后重建 embedded PostgreSQL data 目录；外部数据库只在调用方明确给出连接串和重建意图时处理。

### 4. Validation & Error Matrix

| 条件 | 合同 |
| --- | --- |
| 功能任务 schema 变更 | 新增 migration |
| 修改已提交 migration | 仅限授权 baseline 任务 |
| API 服务启动 | readiness check 通过后装配 repository |
| readiness 缺表 / 缺列 | 启动失败 |
| 替换 baseline 后复用旧 data dir | `_sqlx_migrations` checksum 不匹配，重建 data dir |

### 5. Cases

- Feature: add `0002_<change>.sql`，更新 repository mapping 和 tests。
- Baseline task: document approval -> edit baseline -> rebuild dev DB -> guard with override。
- Seed data: builtin/plugin/library/provider/runtime facts 由 seed 或 repository 写入。

### 6. Tests Required

- `pnpm run migration:guard`。
- 真实 PostgreSQL / embedded PostgreSQL 路径通过 migration runner 初始化。
- 更新 INSERT / SELECT / UPSERT 和 `map_*_row` tests。
- Baseline task 额外验证干净数据库初始化和 data dir 重建说明。

### 7. Boundary / Canonical

```text
feature task -> add NNNN_<change>.sql
```

```text
baseline task -> documented authorization -> edit existing migrations -> rebuild DB -> ALLOW_MIGRATION_BASELINE_REWRITE=1 pnpm run migration:guard
```

---

## Scenario: Cloud Migration Command Boundary

### 1. Scope / Trigger

修改 `agentdash-server serve` / `migrate` / `doctor`、部署命令、Compose / Kubernetes migration job、数据库权限或 readiness 行为。

### 2. Signatures

```text
agentdash-server migrate
agentdash-server serve
agentdash-server doctor
DATABASE_URL
```

### 3. Contracts

- `migrate`: 连接目标 PostgreSQL，运行 migrations，执行 readiness check。
- `serve`: 连接目标 PostgreSQL，只执行 readiness check；通过后装配 repository 和 HTTP router。
- `doctor`: 只执行 readiness check 并输出诊断。
- 最小权限部署可让 `migrate` 使用 DDL 账号，`serve` 使用业务运行账号。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `migrate` 成功 | 输出 `status=ok`、`schema_version`、脱敏数据库描述 |
| `serve` schema 未 ready | 启动失败，不装配 repository |
| `doctor` schema 未 ready | 命令失败并报告 readiness 错误 |
| `DATABASE_URL` 非 PostgreSQL | 配置错误 |

### 5. Cases

- Deploy: `agentdash-server migrate` -> `agentdash-server serve`。
- Dev script: 可显式 migrate 后 serve；外部和 embedded PostgreSQL 仍走同一 migration runner。

### 6. Tests Required

- 修改 API 启动路径: `cargo check -p agentdash-api`。
- 修改 migration: `pnpm run migration:guard` + 真实 PostgreSQL migration runner。
- 修改部署映射: 验证 `migrate` 与 `serve` command 分离。

### 7. Boundary / Canonical

```text
agentdash-server migrate -> run migrations -> check readiness
agentdash-server serve -> check readiness -> start HTTP server
```

---

## Scenario: Migration History Guard

### 1. Scope / Trigger

新增、删除、重命名或修改 `crates/agentdash-infrastructure/migrations/*.sql`。

### 2. Signatures

```text
NNNN_<description>.sql
pnpm run migration:guard
ALLOW_MIGRATION_BASELINE_REWRITE=1
```

### 3. Contracts

- 新 migration 版本号 `NNNN` 大于当前最大已提交版本。
- 普通任务新增 migration 文件。
- Baseline rewrite 任务文档记录：修改范围、重写原因、数据库重建要求、override guard 命令。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| staged diff 新增 migration | 允许 |
| staged diff 修改 / 删除 / rename 已提交 migration | 拒绝 |
| `ALLOW_MIGRATION_BASELINE_REWRITE=1` | 仅用于授权 baseline 任务 |

### 5. Cases

- Add schema: `0002_add_runtime_session_anchor_fks.sql`。
- No migration diff: guard 通过。
- Baseline squash: 授权任务内重写 `0001_init.sql`。

### 6. Tests Required

- 任意 schema 变更运行 `pnpm run migration:guard`。
- 新 migration 通过 migration runner 初始化真实 schema。
- 相关 repository integration / bootstrap readiness 测试通过。

### 7. Boundary / Canonical

```text
feature task -> add crates/agentdash-infrastructure/migrations/NNNN_<change>.sql
```

```text
baseline task -> documented approval -> edit baseline -> rebuild DB -> guard override
```

---

## Scenario: JSONB Document Column Naming

### 1. Scope / Trigger

新增结构化文档列：复杂 value object、owner aggregate document、adapter payload、capability surface、runtime registry。

### 2. Signatures

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL;
```

```rust
use sqlx::types::Json;

query.bind(Json(&run.channel_registry));

let registry: Json<ChannelRegistryDocument> = row.try_get("channel_registry")?;
let registry = registry.0;
```

### 3. Contracts

- 列名表达业务合同；`jsonb` 是存储基质。
- Domain 层定义 typed struct / value object。
- `serde_json::Value` 留在 provider 原始 payload、未知 schema ingress 或调试 envelope 的窄边界。
- 反序列化错误包含真实 `table.column`。
- 高频 predicate、排序、唯一性、claim/lease 字段提升为 scalar 列。
- 需要 document 内查询时，使用明确 `jsonb` operator / expression index。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| 新增复杂值对象列 | 业务语义名 + `jsonb` |
| 历史 `TEXT` JSON 整理 | 迁移为 `jsonb` + typed codec |
| 保留 key 顺序、重复 key 或字节级输入 | design 说明 `json` / `TEXT` 选择 |
| repository 反序列化失败 | `DomainError` 包含 `table.column` |
| document 内字段过滤 | `jsonb` index 或 scalar 列 |

### 5. Cases

- Canonical: `lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL` -> `ChannelRegistryDocument`。
- Historical: `execution_log text` / `activity_state_json text` 留给数据库整理任务。
- Boundary mismatch: `channel_registry text DEFAULT '{}'::text NOT NULL`。
- Boundary mismatch: `channel_registry_jsonb`。

### 6. Tests Required

- Row mapping: default、非空、shape mismatch context。
- Roundtrip: create / update / select。
- Migration: `pnpm run migration:guard`。
- Document query: 覆盖对应 index 查询路径。

### 7. Boundary / Canonical

```rust
let registry: Json<ChannelRegistryDocument> = row.try_get("channel_registry")?;
let registry = registry.0;
```

---

## Scenario: AgentFrame Surface Document And Projection Columns

### 1. Scope / Trigger

AgentFrame capability/context/VFS/MCP/execution/profile surface 或 `agent_frames` schema 变更。

### 2. Signatures

```sql
ALTER TABLE agent_frames
    ADD COLUMN IF NOT EXISTS surface text;

ALTER TABLE agent_frames
    ADD COLUMN hook_plan jsonb;
```

```rust
pub struct AgentFrame {
    pub surface: Option<AgentFrameSurfaceDocument>,
    pub effective_capability_json: Option<Value>,
    pub context_slice_json: Option<Value>,
    pub vfs_surface_json: Option<Value>,
    pub mcp_surface_json: Option<Value>,
    pub execution_profile_json: Option<Value>,
    pub hook_plan: Option<Value>,
}

impl AgentFrame {
    pub fn surface_document(&self) -> AgentFrameSurfaceDocument;
    pub fn apply_surface_projection(&mut self);
    pub fn attach_immutable_hook_plan(&mut self, hook_plan: Value);
}
```

### 3. Contracts

- `agent_frames.surface` 是 frame revision surface 的 canonical document。
- `agent_frames.surface` 当前是既有 `TEXT` JSON schema 事实；新增 adjacent document 按 JSONB 文档列规则设计。
- Split columns 是 repository projection columns；写入从 `surface_document()` 派生，读取时用于迁移物化和 projection 校验。
- `agent_frames.hook_plan`是新revision的immutable HookPlan projection，使用业务语义列名与`jsonb`。它保持nullable以明确表示历史Frame尚未物化；生产writer必须写入typed plan，Runtime读取缺失值时精确失败。
- 最终`hook_plan`列可由后续rename migration建立；已在任一Dashboard或本机Runtime数据库应用的migration内容保持immutable，使所有持久实例通过checksum后顺序收敛到同一schema。
- rename migration只接受单一旧列或单一最终列，并验证最终类型为`jsonb`；双列并存、来源列缺失或类型错误都表示schema事实不一致，应显式失败。
- 新 AgentFrame 写入先填 `surface`，再 `apply_surface_projection()`。
- construction 结束后补挂 HookPlan 时必须调用 `attach_immutable_hook_plan()`：该入口先更新 canonical `surface.hook_plan`，再刷新 split projection；直接写 `frame.hook_plan` 会在下一次 `apply_surface_projection()` 时被 canonical surface 覆盖。
- Backfill migration 从 split columns 物化 `surface`。
- 无 live repository query 的索引用新 migration 删除。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| row 有 `surface` 和 stale split columns | mapper 返回 `surface`，并重新投影 split fields |
| row 无 `surface` 但有 split columns | mapper 从 split columns 物化 `surface` |
| `surface` JSON invalid | mapped `DomainError` 带 `agent_frames.surface` context |
| split projection serialization fails | insert 前返回 mapped `DomainError` |
| 新Frame writer没有提供HookPlan | writer/adoption测试失败；该Frame不得进入Runtime materialization |
| construction 只更新 split `hook_plan` | canonical surface 仍为空，后续 projection 会清除该值；domain/construction port 测试失败 |
| `hook_plan` digest与requirements不匹配 | typed validation error；Host side effect前停止 |
| index 无 live query path | 新 migration 删除，并记录理由 |

### 5. Cases

- Canonical: build `FrameSurfaceDraft` -> write `AgentFrame.surface` -> project split columns。
- Late attachment: `frame.attach_immutable_hook_plan(plan)` -> update canonical surface -> refresh split columns。
- Backfill: split columns -> complete `AgentFrameSurfaceDocument`。
- Boundary mismatch: 只写 `vfs_surface_json`，让 frame surface facts 分裂。

### 6. Tests Required

- Domain: `surface_document()` 与 `apply_surface_projection()`。
- Domain: `attach_immutable_hook_plan()` 后 canonical `surface.hook_plan` 与 split `hook_plan` 完全相等。
- Mapper: surface-overrides-split、split-to-surface materialization。
- Migration guard for `agent_frames` schema change。
- Repository roundtrip preserves canonical surface and projected fields。
- `hook_plan` roundtrip覆盖空requirements与显式ToolBroker requirement；migration guard断言最终列名为`hook_plan jsonb`并覆盖顺序rename migration。

### 7. Boundary / Canonical

```rust
frame.surface = Some(surface_document);
frame.apply_surface_projection();
frame.attach_immutable_hook_plan(validated_hook_plan);
repo.insert_frame(&frame).await?;
```

---

## Scenario: AgentRun Product Command Receipts

### 1. Scope / Trigger

AgentRun 产品命令的 client-command 幂等、结果重放或 accepted Runtime coordinate 持久化发生变化时。

### 2. Signatures

```sql
CREATE TABLE agent_run_product_command_receipts (
    scope_kind text NOT NULL,
    scope_key text NOT NULL,
    client_command_id text NOT NULL,
    runtime_thread_id text,
    runtime_operation_id text,
    UNIQUE (scope_kind, scope_key, client_command_id),
    FOREIGN KEY (runtime_thread_id, runtime_operation_id)
        REFERENCES agent_runtime_operation(thread_id, id)
);
```

```rust
pub struct AgentRunAcceptedRefs {
    pub runtime_thread_id: Option<String>,
    pub runtime_operation_id: Option<String>,
}
```

### 3. Contracts

- Product receipt 只拥有请求 digest、状态、结果重放与产品坐标；Managed Runtime 仍独占 operation/turn 状态。
- accepted Runtime 引用使用 `runtime_thread_id + runtime_operation_id`，复合外键保证 operation 属于同一 thread。
- 纯产品命令可以没有 Runtime operation；Runtime 命令被接受后必须保存原 operation ID。
- `result_json` 保存对调用方返回的 typed response，用于同一 client command 的精确重放。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| 相同 scope/client ID 与相同 digest | 返回首次 receipt/result，不重复 side effect |
| 相同 scope/client ID 与不同 digest | typed digest conflict |
| operation ID 不属于给定 thread | 复合外键拒绝整个 accepted update |
| Runtime command accepted 但 receipt 未保存 operation | API/application 测试失败，不得伪造 protocol turn ID |
| receipt schema 缺失 | readiness 失败，不装配 repository |

### 5. Cases

- Good: compact command -> Runtime operation receipt -> product receipt stores thread/operation -> duplicate replays result。
- Base: mailbox reorder has no Runtime operation and stores only product refs/result。
- Bad: 把 Runtime operation ID 编码为 `protocol_turn_id`，或恢复已退役 RuntimeSession FK。

### 6. Tests Required

- Repository: claim/duplicate/digest conflict、accepted refs/result roundtrip、复合 FK mismatch。
- Migration: fresh embedded PostgreSQL 与既有 schema 顺序升级后新表 ready，旧 `agent_run_command_receipts` 仍 retired。
- API: cancel/compaction 接受后保存真实 operation ID；重复 client command 不产生第二 operation。

### 7. Boundary / Canonical

```text
AgentRun product command -> product receipt claim -> Managed Runtime command
                         -> accepted thread/operation refs + typed result
                         -> duplicate result replay
```

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`，参数数量匹配。
- `SELECT ... INTO` 后检查 `FOUND`。
- JSONB 数组遍历使用 `jsonb_array_elements()`。
- 迁移脚本保持幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`。
