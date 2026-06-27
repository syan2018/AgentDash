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
- 复杂值对象以 JSON 文本存入 `TEXT`
- 新增目标模型列不因为 JSON 文本存储而追加 `_json` / `_jsonb` 后缀；列名优先表达业务语义，例如 `lifecycle_runs.context`、`lifecycle_runs.orchestrations`、`lifecycle_runs.view_projection`
- 时间字段使用 PostgreSQL 原生 timestamp 类型，repository 直接 bind/read `chrono::DateTime<Utc>`
- Repository 实现模式详见 [repository-pattern.md](./repository-pattern.md)
- 显式 `agentdash-server migrate` 入口负责运行 PostgreSQL migrations；API 长驻服务启动在 repository 装配前只执行 schema readiness 检查。这样发布流程可以把 schema 演进放进一次性部署步骤，并让长期服务只依赖运行期所需数据库权限。

---

## 事务规则

- **单一聚合**：事务边界由对应 Repository 负责（如 `WorkspaceRepository` 内部同事务写 `workspaces` + `workspace_bindings`，`LifecycleRunRepository` 整体写回 lifecycle context / orchestrations / tasks / view projection）
- **跨聚合**：使用显式 Command Port 或 Unit of Work，不要硬塞进单一 Repository trait
- Story projection 与 LifecycleRun Task facts 同时变化时，使用应用层命令编排多个聚合；不要让 `StoryRepository` 承担 Task durable CRUD

---

## Schema 事实源

### PostgreSQL

业务库的 schema 事实源是 `crates/agentdash-infrastructure/migrations/`。日常 schema 变更按正常 migration 链新增文件推进，原因是 migration 历史是仓库内可审计的结构演进事实，开发期本地库、测试库和 embedded PostgreSQL 都应观察同一条递进路径。

已提交的 migration 文件是历史事实，日常 feature / bugfix / refactor 任务严禁修改、删除或重命名，包括当前 baseline `0001_init.sql`。预研期“不保留旧兼容路径”只表示新 migration 可以直接把 schema 推到正确目标，不表示可以重写历史 migration。只有明确授权的数据库 baseline squash / reset / merge 任务可以修改既有 migration；该任务必须在 `prd.md` / `design.md` 写明授权范围、重建数据库要求和验证命令。

Repository 启动逻辑只观察已迁移 schema。API bootstrap 不调用 PostgreSQL repository schema 初始化；需要直接构造 `AppState` 或 repository 的测试路径也先运行 migrations，再执行 readiness 检查。Repository 可以保留无 DDL 的 readiness helper，但不能创建表、补列、建索引或执行 schema 数据迁移。

预研期允许在明确的数据库 baseline squash / reset / merge 时间点压缩 PostgreSQL migration 基线。阶段性 squash 时整理 `0001_init.sql` 表达当前正确 schema，避免开发期重命名、回填和旧模型迁移长期分散当前事实。`0001_init.sql` 应保持为手工整理后的 schema baseline：只保留 DDL、约束、索引、序列和必要扩展，不保留 pg_dump header、object comments、`public.` 前缀噪音、回填默认值或旧约束命名。进入需要保留真实环境数据的阶段后，migration 历史转为增量审计事实，不再随意压缩。

初始化 migration 只表达 schema、约束、索引和必要扩展。Builtin / Plugin Shared Library assets、LLM Provider、auth session、settings、backend registration、runtime health、session / lifecycle runtime facts 都由启动期 seed、API use case 或 runtime repository 写入，原因是这些数据随代码、插件、用户配置或运行状态变化，不属于 schema 基线。

只有执行 migration squash 或替换基线后，embedded PostgreSQL 物理 data 目录需要重建。SQLx 通过 `_sqlx_migrations` 记录 migration version 和 checksum；替换 migration 文件后复用旧数据库会让 bookkeeping 与新基线不一致。外部 `DATABASE_URL` 指向的数据库只在调用方明确给出目标连接串和重建意图时处理。

### 本机 Embedded PostgreSQL

本机 session runtime 使用 embedded PostgreSQL，并复用同一套 migration 与 readiness 检查。这样本机恢复路径和云端 session persistence 观察同一份 schema contract，避免为本机维护第二套 schema 演进机制。

### Checklist

- [ ] PostgreSQL 新增 migration 文件
- [ ] `pnpm run migration:guard` 通过；如果修改既有 migration，当前任务必须是明确授权的 baseline squash / reset / merge
- [ ] PostgreSQL integration / bootstrap / local embedded runtime 路径通过 migration runner 初始化真实 schema
- [ ] 更新 INSERT/SELECT/UPSERT 语句和 `map_*_row` 函数
- [ ] 更新测试代码

### 删除旧列

- Repository 主线不再读写旧列
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
- baseline squash / reset / merge 任务必须额外验证干净数据库初始化，并记录旧 embedded PostgreSQL data 目录重建要求。

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

## Scenario: JSON Text Column Naming

### 1. Scope / Trigger

- Trigger: 新增 `TEXT` 列承载复杂值对象 JSON 序列化。
- Scope: migration、repository row mapping、错误上下文和后续 spec / task 文档命名。

### 2. Signatures

新增目标列优先使用业务语义名：

```sql
ALTER TABLE lifecycle_runs
    ADD COLUMN IF NOT EXISTS context text DEFAULT '{}'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS orchestrations text DEFAULT '[]'::text NOT NULL,
    ADD COLUMN IF NOT EXISTS view_projection text;
```

Repository 仍用 JSON 序列化读写：

```rust
serde_json::to_string(&run.context)?;
parse_json_column::<LifecycleContext>(&row.context, "lifecycle_runs.context")?;
```

### 3. Contracts

- 列名表达业务合同，JSON 文本只是当前 PostgreSQL 存储方式。
- 错误上下文使用真实列名，例如 `lifecycle_runs.orchestrations`。
- 已存在的历史列名保持为迁移事实；新增目标列按当前命名规则落地。

### 4. Validation & Error Matrix

| 条件 | 结果 |
| --- | --- |
| 新增复杂值对象列 | 使用业务语义名和 `TEXT` 类型 |
| repository 解析失败 | `DomainError` 包含真实 `table.column` |
| 修改 migration 历史列名 | 只有明确 baseline squash / reset / merge 任务可以做 |

### 5. Good/Base/Bad Cases

- Good: `lifecycle_runs.orchestrations text DEFAULT '[]'::text NOT NULL`。
- Base: 旧 schema 中已有 `activity_state_json`，作为历史事实保留。
- Bad: 新目标列写成 `orchestrations_json`，会把存储方式伪装成领域概念。

### 6. Tests Required

- Repository row mapping 测试覆盖默认 JSON 文本和坏 JSON 错误上下文。
- Repository roundtrip 测试覆盖 create / update / select。
- 任意新增 migration 运行 `pnpm run migration:guard`。

### 7. Wrong vs Correct

#### Wrong

```sql
ADD COLUMN orchestrations_json text DEFAULT '[]'::text NOT NULL;
```

#### Correct

```sql
ADD COLUMN orchestrations text DEFAULT '[]'::text NOT NULL;
```

---

## PL/pgSQL 迁移脚本要点

- `RAISE` 占位符是单个 `%`（不是 `%%`），参数数量必须与占位符数量一致
- `SELECT ... INTO` 后必须检查 `FOUND`
- JSONB 数组遍历用 `jsonb_array_elements()`，不用 `FOREACH ... IN ARRAY`
- 迁移脚本必须幂等：`ADD COLUMN IF NOT EXISTS`、`ON CONFLICT DO NOTHING`
