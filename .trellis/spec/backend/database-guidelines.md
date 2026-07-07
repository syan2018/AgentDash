# 数据库规范

> PostgreSQL + SQLx（云端与本机嵌入式运行时）。

---

## 存储分层

| 层 | 技术 | 职责 |
|----|------|------|
| 云端 | PostgreSQL + SQLx | 业务数据（Project/Story/Workspace/Session 等） |
| 本机 | Embedded PostgreSQL + SQLx | 本机 session runtime 持久化与恢复 |

---

## 核心约定

- 基础设施层错误必须转换为 `DomainError`，不泄露 `sqlx::Error`
- PostgreSQL repository 统一通过 `persistence::postgres::db_err` / `sql_err_for` 映射 SQLx 错误，保留 NotFound、Conflict、Database 三类可映射语义
- 数据库列名和 JSON 序列化统一 `snake_case`
- 新增结构化文档列默认使用 PostgreSQL `jsonb`，并在 domain / repository 边界映射为 typed value object
- 既有 `TEXT` 承载 JSON 的列是历史 schema 事实；新增模型按 `jsonb` 设计，迁移清理只在明确数据库整理任务中处理
- 新增文档列不因为 `jsonb` 存储而追加 `_json` / `_jsonb` 后缀；列名优先表达业务语义，例如 `lifecycle_runs.orchestrations`、`lifecycle_runs.tasks`、`lifecycle_runs.execution_log`
- `json` 只用于已验证为 JSON 且业务要求保留 key 顺序、重复 key 或原始 JSON 文本形态的少数列；一般业务文档不使用 `json`
- 高频过滤、排序、权限判断、claim/lease 状态使用 PostgreSQL 原生 scalar 列；低频 owner-local document 使用 `jsonb`
- 时间字段使用 PostgreSQL 原生 timestamp 类型，repository 直接 bind/read `chrono::DateTime<Utc>`
- Repository 实现模式详见 [repository-pattern.md](./repository-pattern.md)
- 显式 `agentdash-server migrate` 入口负责运行 PostgreSQL migrations；API 长驻服务启动在 repository 装配前只执行 schema readiness 检查。这样发布流程可以把 schema 演进放进一次性部署步骤，并让长期服务只依赖运行期所需数据库权限。

---

## 事务规则

- **单一聚合**：事务边界由对应 Repository 负责（如 `WorkspaceRepository` 内部同事务写 `workspaces` + `workspace_bindings`，`LifecycleRunRepository` 整体写回 orchestrations / tasks / execution log）
- **跨聚合**：使用显式 Command Port 或 Unit of Work，不要硬塞进单一 Repository trait
- Story projection 与 LifecycleRun Task facts 同时变化时，使用应用层命令编排多个聚合；不要让 `StoryRepository` 承担 Task durable CRUD

---

## Scenario: Agent Runtime Fact Storage Boundary

### 1. Scope / Trigger

- Trigger: 新增 Agent / Lifecycle / Project runtime fact、临时通信关系、inbox/outbox、capability projection source、adapter binding、delivery planning 或 scheduler 状态。
- Scope: domain entity/value object、repository trait、PostgreSQL migration、application service、recovery/query tests。

Agent 业务事实先按 owner、生命周期和恢复边界建模，再决定物理存储。PostgreSQL 可以作为 owner aggregate document store 使用；关系表只表达确实脱离 owner document 的执行或查询语义。

### 2. Signatures

Owner document column 使用业务语义名：

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL;
```

Owner aggregate 持有 typed document：

```rust
pub struct LifecycleRun {
    pub channel_registry: ChannelRegistryDocument,
}
```

Repository 只负责 JSONB typed roundtrip 和错误上下文：

```rust
use sqlx::types::Json;

let registry: Json<ChannelRegistryDocument> = row.try_get("channel_registry")?;
let registry = registry.0;

query
    .bind(Json(&run.channel_registry));
```

### 3. Contracts

- 事实的默认归属由业务 owner 决定：Project-owned fact 跟 Project 资产/配置 owner 走，LifecycleRun-scoped fact 跟 LifecycleRun 走，AgentRun-scoped fact 跟 run + agent owner 走。
- Owner-local、随 owner 生灭、只在 owner 范围查询的事实，优先作为 owner document 的 typed value object 保存。
- 多 producer fan-in 不自动构成独立表理由；如果所有 producer 都写入同一个 owner 的当前输入/状态，application service 可以在 owner repository 事务内合并。
- Claim / lease 不自动构成独立表理由；如果消费语义可以按 owner 串行化、按 owner version/CAS 更新，或只需要恢复 owner 的当前 pending document，就保留 owner document。
- 独立表需要清晰承担至少一种 owner document 难以表达的语义：跨 owner 全局调度扫描、同一 owner 内高并发多 worker 抢占、单条事实独立 retention/audit、跨 owner 查询索引、或必须由数据库唯一约束保护的跨聚合不变量。
- Owner document 物理列默认使用 `jsonb`；`TEXT` JSON 只作为既有 migration 的历史形态读取，不作为新增 schema 形态。
- Repository 使用 `sqlx::types::Json<T>` 或共享 JSONB codec 读写 typed document；业务代码不直接传播未建模的 `serde_json::Value`。
- Capability projection 不是事实源；projection 从 owner facts 派生，并可重建。
- 既有 `lifecycle_gates`、`agent_run_mailbox_messages`、`agent_run_lineages` 等表不是新模型的默认先例；后续清理或扩展时按同一判断矩阵重新评估。

### 4. Validation & Error Matrix

| 条件 | 存储选择 |
| --- | --- |
| fact 随单个 owner 创建、恢复、删除 | owner document |
| fact 只被 owner-scoped API / service 查询 | owner document |
| fact 是 capability / prompt / UI 可见面的输入 | owner document -> projector |
| fact 需要跨 owner 按状态扫描调度 | 独立 store candidate |
| fact 需要多 worker 对单条 item 抢占 claim | 独立 store candidate |
| fact 需要 owner 删除后仍保留审计 | 独立 store candidate |
| fact 需要跨 Project / LifecycleRun 查询索引 | 独立 store candidate |
| `jsonb` document 反序列化失败 | repository 返回带 `table.column` context 的 mapped `DomainError` |
| document schema 演进 | 使用 `serde(default)`、schema version 或 mapper materialization 保持旧 row 可读 |
| 需要按 document 内字段查询或排序 | 建立明确 `jsonb` operator / expression index，或提升为 scalar 列 |

### 5. Good/Base/Bad Cases

- Good: runtime channel registry 保存在 `LifecycleRun.channel_registry`，因为它随 LifecycleRun 生灭，并由 ChannelService 在 run owner 内读写。
- Good: Project 公共 Channel 在当前任务中只定义 owner store port，因为后续 Project Assets 系统才是物理承载决策点。
- Base: 一个真正需要跨 run 扫描、抢占 claim、恢复未完成 item 的后台队列可以使用独立表，但任务文档必须写出这些执行语义。
- Bad: 一个只在 AgentRun 工作台展示和消费的 pending input 因为叫 mailbox 就天然拆成表；命名不能替代生命周期和并发语义论证。

### 6. Tests Required

- Owner document repository roundtrip 覆盖 default document、非空 document、document shape mismatch 错误上下文。
- 新 document 字段 migration 运行 `pnpm run migration:guard`，并覆盖干净数据库初始化。
- Application service tests 覆盖 owner document 更新只通过 owner repository / service 边界发生。
- Projection tests 覆盖 projection 从 owner document 派生，且不会反向写 projection 作为事实源。
- 若选择独立表，repository tests 必须覆盖它声称承担的独立语义：跨 owner scan、claim/lease、retention/audit、唯一约束或跨 owner query。

### 7. Boundary / Canonical

#### Boundary Mismatch

```text
runtime relation scoped to one LifecycleRun
  -> separate relation table
  -> cleanup/index/join semantics added after the fact
```

#### Canonical

```text
runtime relation scoped to one LifecycleRun
  -> LifecycleRun document
  -> application service updates typed owner value object
  -> owner deletion removes the runtime fact
```

#### Independent Store Candidate

```text
global worker queue
  -> item store with status, claim_token, claim_expires_at
  -> workers scan by status/lease/order across owners
  -> recovery scans incomplete claims without loading every owner document
```

---

## Schema 事实源

### PostgreSQL

业务库的 schema 事实源是 `crates/agentdash-infrastructure/migrations/`。日常 schema 变更按正常 migration 链新增文件推进，原因是 migration 历史是仓库内可审计的结构演进事实，开发期本地库、测试库和 embedded PostgreSQL 都应观察同一条递进路径。

已提交的 migration 文件是历史事实，日常 feature / bugfix / refactor 任务严禁修改、删除或重命名，包括当前 baseline `0001_init.sql`。预研期 schema 直接推进到正确目标，不表示可以重写历史 migration。只有明确授权的数据库 baseline squash / reset / merge 任务可以修改既有 migration；该任务必须在 `prd.md` / `design.md` 写明授权范围、重建数据库要求和验证命令。

Repository 启动逻辑只观察已迁移 schema。API bootstrap 不调用 PostgreSQL repository schema 初始化；需要直接构造 `AppState` 或 repository 的测试路径也先运行 migrations，再执行 readiness 检查。Repository 可以保留无 DDL 的 readiness helper，但不能创建表、补列、建索引或执行 schema 数据迁移。

预研期允许在明确的数据库 baseline squash / reset / merge 时间点压缩 PostgreSQL migration 基线。阶段性 squash 时整理 `0001_init.sql` 表达当前正确 schema，避免开发期重命名、回填和过往模型迁移长期分散当前事实。`0001_init.sql` 应保持为手工整理后的 schema baseline：只保留 DDL、约束、索引、序列和必要扩展，不保留 pg_dump header、object comments、`public.` 前缀噪音、回填默认值或历史约束命名。进入需要保留真实环境数据的阶段后，migration 历史转为增量审计事实，不再随意压缩。

初始化 migration 只表达 schema、约束、索引和必要扩展。Builtin / Plugin Shared Library assets、LLM Provider、auth session、settings、backend registration、runtime health、session / lifecycle runtime facts 都由启动期 seed、API use case 或 runtime repository 写入，原因是这些数据随代码、插件、用户配置或运行状态变化，不属于 schema 基线。

只有执行 migration squash 或替换基线后，embedded PostgreSQL 物理 data 目录需要重建。SQLx 通过 `_sqlx_migrations` 记录 migration version 和 checksum；替换 migration 文件后复用既有数据库会让 bookkeeping 与新基线不一致。外部 `DATABASE_URL` 指向的数据库只在调用方明确给出目标连接串和重建意图时处理。

### 本机 Embedded PostgreSQL

本机 session runtime 使用 embedded PostgreSQL，并复用同一套 migration 与 readiness 检查。这样本机恢复路径和云端 session persistence 观察同一份 schema contract，避免为本机维护第二套 schema 演进机制。

### Checklist

- [ ] PostgreSQL 新增 migration 文件
- [ ] `pnpm run migration:guard` 通过；如果修改既有 migration，当前任务必须是明确授权的 baseline squash / reset / merge
- [ ] PostgreSQL integration / bootstrap / local embedded runtime 路径通过 migration runner 初始化真实 schema
- [ ] 更新 INSERT/SELECT/UPSERT 语句和 `map_*_row` 函数
- [ ] 更新测试代码

### 删除退役列

- Repository 主线不再读写退役列
- PostgreSQL 新增 migration 用 `DROP COLUMN IF EXISTS`
- 阶段性 squash 后，基线 migration 与当前 schema 目标保持一致

## Scenario: Cloud Migration Command Boundary

### 1. Scope / Trigger

- Trigger: 修改云端服务启动、部署命令、Compose / Kubernetes migration job、数据库连接权限或 schema readiness 行为。
- Scope: `agentdash-server serve`、`agentdash-server migrate`、`agentdash-server doctor`、PostgreSQL migration runner、API repository bootstrap。

### 2. Signatures

```text
agentdash-server serve
agentdash-server migrate
agentdash-server doctor
```

### 3. Contracts

- `migrate` 连接部署目标 PostgreSQL，运行 `crates/agentdash-infrastructure/migrations/` 中的 SQLx migrations，然后执行 schema readiness 检查。
- `serve` 连接部署目标 PostgreSQL，只执行 schema readiness 检查；检查通过后再装配 repository 和 HTTP router。
- `doctor` 连接部署目标 PostgreSQL，只执行 schema readiness 检查，并输出诊断报告。
- `DATABASE_URL` 是部署期 PostgreSQL 连接串。需要最小权限部署时，`migrate` 可使用具备 DDL 权限的连接串，`serve` 使用业务运行权限连接串。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| `migrate` 连接成功且 schema 可迁移 | 输出 `status = ok`、`schema_version` 和脱敏后的数据库描述 |
| `serve` 发现 schema 缺表或未迁移 | 启动失败，不装配 repository |
| `doctor` 发现 schema 未 ready | 命令失败并报告 readiness 错误 |
| `DATABASE_URL` 非 PostgreSQL 协议 | 命令失败并报告配置错误 |

### 5. Good/Base/Bad Cases

- Good: 发布流程先运行 `agentdash-server migrate`，成功后启动 `agentdash-server serve`。
- Base: 开发启动脚本可以先显式调用 `agentdash-server migrate`，再启动 `agentdash-server serve`；外部 PostgreSQL 与 embedded PostgreSQL 仍复用同一套 migrations 与 readiness 检查。
- Bad: 长驻服务启动时运行 migrations，使多副本部署和最小权限数据库账号无法形成清晰边界。

### 6. Tests Required

- 修改 `agentdash-api` 启动路径时至少运行 `cargo check -p agentdash-api`。
- 修改 migration 文件时运行 `pnpm run migration:guard`，并用真实 PostgreSQL 路径验证 migration runner。
- 修改部署脚本或 Compose / Kubernetes 映射时验证 `migrate` 与 `serve` 命令分别使用预期 command。

### 7. Wrong vs Correct

#### Wrong

```text
agentdash-server serve -> run migrations -> start HTTP server
```

#### Correct

```text
agentdash-server migrate -> run migrations -> check schema readiness
agentdash-server serve -> check schema readiness -> start HTTP server
```

## Scenario: Migration History Guard

### 1. Scope / Trigger

- Trigger: 任意新增、删除、重命名或修改 `crates/agentdash-infrastructure/migrations/*.sql`。
- Scope: PostgreSQL migration 历史、embedded PostgreSQL 初始化、repository readiness 和 CI / pre-commit guard。

### 2. Signatures

- 新增 migration 文件命名：`NNNN_<description>.sql`，`NNNN` 必须大于当前最大已提交 migration version。
- 守卫命令：

```powershell
pnpm run migration:guard
```

- 授权 baseline rewrite 时才允许：

```powershell
$env:ALLOW_MIGRATION_BASELINE_REWRITE='1'; pnpm run migration:guard
```

### 3. Contracts

- 普通任务只能新增 migration 文件；不能修改、删除或重命名已提交 migration。
- 已提交 migration 的定义以 `HEAD` 中存在的 `crates/agentdash-infrastructure/migrations/*.sql` 为准。
- baseline squash / reset / merge 任务必须在任务文档中写明：
  - 修改哪些既有 migration。
  - 为什么当前时间点需要重写 baseline。
  - 哪些数据库目录或外部数据库需要重建。
  - 允许使用 `ALLOW_MIGRATION_BASELINE_REWRITE=1` 的验证命令。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| staged diff 新增 `crates/agentdash-infrastructure/migrations/0002_x.sql` | 允许 |
| staged diff 修改 `crates/agentdash-infrastructure/migrations/0001_init.sql` | 拒绝 |
| staged diff 删除已提交 migration | 拒绝 |
| staged diff rename 已提交 migration | 拒绝 |
| `ALLOW_MIGRATION_BASELINE_REWRITE=1` | 允许，但只用于已授权 baseline 任务 |

### 5. Good/Base/Bad Cases

- Good: `0002_add_runtime_session_anchor_fks.sql` 新增 FK，repository 和 tests 同步更新。
- Base: 没有 migration diff，guard 直接通过。
- Bad: 在功能任务中直接修改 `0001_init.sql` 增加 FK；这会让本地库、测试库和任务审计观察到不同历史。

### 6. Tests Required

- 任意数据库 schema 变更 PR 必须运行 `pnpm run migration:guard`。
- 新 migration 必须由 migration runner 初始化真实 schema，并通过相关 repository integration 或 bootstrap readiness 测试。
- baseline squash / reset / merge 任务必须额外验证干净数据库初始化，并记录既有 embedded PostgreSQL data 目录重建要求。

### 7. Wrong vs Correct

#### Wrong

```text
feature task -> edit crates/agentdash-infrastructure/migrations/0001_init.sql
```

#### Correct

```text
feature task -> add crates/agentdash-infrastructure/migrations/0002_<change>.sql
```

#### Correct Only In Authorized Baseline Task

```text
database baseline squash task -> document approval -> edit 0001_init.sql -> rebuild dev DB -> run guard with ALLOW_MIGRATION_BASELINE_REWRITE=1
```

## Scenario: JSONB Document Column Naming

### 1. Scope / Trigger

- Trigger: 新增结构化文档列承载复杂 value object、owner aggregate document、adapter payload、capability surface 或 runtime registry。
- Scope: migration、repository row mapping、错误上下文和后续 spec / task 文档命名。

### 2. Signatures

新增目标列优先使用业务语义名：

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL;
```

Repository 使用 JSONB typed codec 读写：

```rust
use sqlx::types::Json;

query.bind(Json(&run.channel_registry));

let registry: Json<ChannelRegistryDocument> = row.try_get("channel_registry")?;
let registry = registry.0;
```

### 3. Contracts

- 列名表达业务合同，`jsonb` 是 PostgreSQL 文档存储基质。
- 新增结构化文档列默认使用 `jsonb`；只有存在保留原始 JSON 文本顺序、重复 key 或字节级输入的明确需求时，才在 design 中说明 `json` / `TEXT` 选择。
- 复杂文档在 domain 层必须有 typed struct / value object；`serde_json::Value` 只留在 provider 原始 payload、未知 schema ingress 或调试 envelope 的窄边界。
- 错误上下文使用真实列名，例如 `lifecycle_runs.orchestrations`。
- 已存在的历史 `TEXT` JSON 列保持为迁移事实；新增目标列按当前 `jsonb` 规则落地，批量转换由明确数据库整理任务处理。
- 高频 predicate、排序、唯一性和 claim/lease 字段提升为 scalar 列；document 内查询只在有明确 query plan 时使用 `jsonb` operator / expression index。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| 新增复杂值对象列 | 使用业务语义名和 `jsonb` 类型 |
| 从历史 `TEXT` JSON 迁移结构化业务文档 | 迁移为 `jsonb`，repository 改为 typed `Json<T>` / codec |
| 需要保留 JSON key 顺序或重复 key 语义 | 使用 `json`，并在 design 写明业务原因 |
| repository 反序列化失败 | `DomainError` 包含真实 `table.column` |
| 需要 document 内字段过滤 | 使用明确 `jsonb` index，或提升为 scalar 列 |
| 需要字节级原始 payload | 在 design 中记录选择 `json` / `TEXT` 的业务原因 |
| 修改 migration 历史列名 | 只有明确 baseline squash / reset / merge 任务可以做 |

### 5. Good/Base/Bad Cases

- Good: `lifecycle_runs.channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL`，repository 映射为 `ChannelRegistryDocument`。
- Base: 既有 schema 中已有 `execution_log text` 或 `activity_state_json text`，作为历史事实保留到数据库整理任务。
- Bad: 新目标列写成 `channel_registry text DEFAULT '{}'::text NOT NULL`，会把结构化文档重新降级成字符串协议。
- Bad: 新目标列写成 `channel_registry_jsonb`，会把存储方式伪装成领域概念。

### 6. Tests Required

- Repository row mapping 测试覆盖默认 document、非空 document 和 shape mismatch 错误上下文。
- Repository roundtrip 测试覆盖 create / update / select。
- 任意新增 migration 运行 `pnpm run migration:guard`。
- 需要 document 内查询时，测试覆盖对应 `jsonb` operator / expression index 的查询路径。

### 7. Wrong vs Correct

#### Wrong

```sql
ADD COLUMN channel_registry text DEFAULT '{}'::text NOT NULL;
```

#### Correct

```sql
ADD COLUMN channel_registry jsonb DEFAULT '{}'::jsonb NOT NULL;
```

#### Boundary Mismatch

```rust
let raw: String = row.try_get("channel_registry")?;
let registry: ChannelRegistryDocument = serde_json::from_str(&raw)?;
```

#### Canonical

```rust
let registry: Json<ChannelRegistryDocument> = row.try_get("channel_registry")?;
let registry = registry.0;
```

## Scenario: AgentFrame Surface Document And Projection Columns

### 1. Scope / Trigger

- Trigger: AgentFrame capability/context/VFS/MCP/execution/profile surface changes or schema changes around `agent_frames`.
- Scope: `agent_frames.surface`, split projection columns, repository row mapping, migration backfill, AgentFrame repository tests.

### 2. Signatures

既有 schema 事实：

```sql
ALTER TABLE agent_frames
    ADD COLUMN IF NOT EXISTS surface text;
```

```rust
pub struct AgentFrame {
    pub surface: Option<AgentFrameSurfaceDocument>,
    pub effective_capability_json: Option<Value>,
    pub context_slice_json: Option<Value>,
    pub vfs_surface_json: Option<Value>,
    pub mcp_surface_json: Option<Value>,
    pub execution_profile_json: Option<Value>,
}

impl AgentFrame {
    pub fn surface_document(&self) -> AgentFrameSurfaceDocument;
    pub fn apply_surface_projection(&mut self);
}
```

### 3. Contracts

- `agent_frames.surface` 是 frame revision surface 的 canonical document。
- `agent_frames.surface` 当前是既有 `TEXT` JSON schema 事实；新增 AgentFrame adjacent document 或后续整理任务按上文 JSONB document 规则设计。
- split columns 是 repository projection columns；写入时从 `surface_document()` 派生，读取时只用于迁移物化和 projection 校验。
- 新 AgentFrame 写入先填充 `surface`，再调用 projection 逻辑后 insert。
- backfill migration 从既有 split columns 派生 `surface`，让历史 rows 仍可读取。
- 没有 live repository query 的索引通过新 migration 删除；保留物理表需要有独立查询、独立更新或重建成本理由。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| row has `surface` and stale split columns | mapper 返回 `surface` document，并用它重新投影 split fields |
| row has no `surface` but has split columns | mapper 从 split columns 物化 `surface`，用于验证迁移 backfill 覆盖 |
| `surface` JSON is invalid | repository 返回带 `agent_frames.surface` context 的 mapped `DomainError` |
| split projection serialization fails | repository 在 insert 前返回 mapped `DomainError` |
| index has no live query path | 通过新 migration 删除，并在工作项 ledger 记录理由 |

### 5. Good/Base/Boundary Cases

- Good: frame construction builds `FrameSurfaceDraft`, writes `AgentFrame.surface`, and repository projects split columns for existing read helpers.
- Base: migration backfill row materializes a complete `AgentFrameSurfaceDocument` from split columns.
- Boundary mismatch: code writes only `vfs_surface_json` and leaves `surface` absent, causing launch/query/context delivery to read different surface facts.

### 6. Tests Required

- Domain tests cover `surface_document()` split-column materialization and `apply_surface_projection()`.
- PostgreSQL mapper tests cover surface-overrides-split and split-to-surface materialization.
- Migration guard runs for any `agent_frames` schema change.
- Repository roundtrip tests assert insert/select preserves canonical surface and projected fields.

### 7. Boundary / Canonical

#### Boundary

```rust
frame.vfs_surface_json = Some(vfs_json);
frame.mcp_surface_json = Some(mcp_json);
repo.insert_frame(&frame).await?;
```

#### Canonical

```rust
frame.surface = Some(surface_document);
frame.apply_surface_projection();
repo.insert_frame(&frame).await?;
```

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`（不是 `%%`），参数数量必须与占位符数量一致
- `SELECT ... INTO` 后必须检查 `FOUND`
- JSONB 数组遍历用 `jsonb_array_elements()`，不用 `FOREACH ... IN ARRAY`
- 迁移脚本必须幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`
