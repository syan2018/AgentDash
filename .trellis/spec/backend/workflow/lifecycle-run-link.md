# LifecycleRunLink Contract

> LifecycleRun 与业务对象的显式关联层。替代 SessionBinding 的隐式反查路径。

---

## Scenario: Run-Subject Association & Session Demotion

### 1. Scope / Trigger

- 新增 `lifecycle_run_links` 数据库表及 `LifecycleRunLink` domain entity
- 删除 `session_bindings` 表，`Session` 降级为纯 runtime event stream 容器
- 新增 `sessions.project_id` 列替代通过 binding 间接查找项目归属
- 新增 run-oriented API endpoints（`/stories/{id}/runs`、`/lifecycle-runs/{id}/links`）
- 新增 `CapabilityScope` / `CapabilityScopeCtx` 替代 `SessionOwnerType`

### 2. Signatures

#### Domain Entity

```rust
// crates/agentdash-domain/src/workflow/run_link.rs
pub struct LifecycleRunLink {
    pub id: Uuid,
    pub run_id: Uuid,
    pub subject_kind: RunLinkSubjectKind,
    pub subject_id: Uuid,
    pub role: RunLinkRole,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

pub enum RunLinkSubjectKind {
    Story, Project, RoutineExecution, Task, LifecycleRun, External,
}

pub enum RunLinkRole {
    Source,           // Run 的触发来源
    Subject,         // Run 正在处理的对象
    ProjectionTarget,// Run 输出投影目标
    ControlScope,    // Permission System 授权的 scope
    SpawnedBy,       // 父 run lineage
}
```

#### Repository Trait

```rust
#[async_trait]
pub trait LifecycleRunLinkRepository: Send + Sync {
    async fn create(&self, link: &LifecycleRunLink) -> Result<(), DomainError>;
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<LifecycleRunLink>, DomainError>;
    async fn list_by_subject(&self, kind: RunLinkSubjectKind, id: Uuid) -> Result<Vec<LifecycleRunLink>, DomainError>;
    async fn list_by_subject_and_role(&self, kind: RunLinkSubjectKind, id: Uuid, role: RunLinkRole) -> Result<Vec<LifecycleRunLink>, DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    async fn delete_by_run(&self, run_id: Uuid) -> Result<(), DomainError>;
}
```

#### Application Service

```rust
// crates/agentdash-application/src/workflow/run_link_service.rs
pub struct LifecycleRunLinkService {
    link_repo: Arc<dyn LifecycleRunLinkRepository>,
    run_repo: Arc<dyn LifecycleRunRepository>,
}

impl LifecycleRunLinkService {
    pub async fn attach_subject(&self, run_id, subject_kind, subject_id, role) -> Result<LifecycleRunLink, _>;
    pub async fn list_runs_for_story(&self, story_id: Uuid) -> Result<Vec<LifecycleRun>, _>;
    pub async fn active_run_for_story(&self, story_id: Uuid) -> Result<Option<LifecycleRun>, _>;
    pub async fn list_runs_for_subject(&self, kind, id) -> Result<Vec<LifecycleRun>, _>;
    pub async fn list_subjects_for_run(&self, run_id: Uuid) -> Result<Vec<LifecycleRunLink>, _>;
    pub async fn list_links_by_subject_and_role(&self, kind, id, role) -> Result<Vec<LifecycleRunLink>, _>;
}
```

#### Database Schema

```sql
-- migrations/0069_lifecycle_run_links.sql
CREATE TABLE IF NOT EXISTS lifecycle_run_links (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    subject_kind TEXT NOT NULL,      -- story / project / routine_execution / task / lifecycle_run / external
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,              -- source / subject / projection_target / control_scope / spawned_by
    metadata TEXT,                   -- JSON or NULL
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Indices
CREATE INDEX idx_lifecycle_run_links_run_id ON lifecycle_run_links(run_id);
CREATE INDEX idx_lifecycle_run_links_subject ON lifecycle_run_links(subject_kind, subject_id);
CREATE INDEX idx_lifecycle_run_links_subject_role ON lifecycle_run_links(subject_kind, subject_id, role);
```

```sql
-- migrations/0070_drop_session_bindings.sql
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS project_id TEXT;
-- 数据回填 project_id from session_bindings
DROP TABLE IF EXISTS session_bindings;
```

#### REST API

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/stories/{story_id}/runs` | `list_story_runs` | Story 的所有关联 runs |
| GET | `/stories/{story_id}/runs/active` | `get_active_story_run` | Story 当前活跃 run |
| GET | `/lifecycle-runs/{id}/links` | `list_run_links` | Run 的所有 links |
| POST | `/lifecycle-runs/{id}/links` | `attach_run_link` | 创建 run-subject link |

#### CapabilityScope (SPI)

```rust
// crates/agentdash-spi/src/platform/tool_capability.rs
pub enum CapabilityScope { Project, Story, Task }

pub enum CapabilityScopeCtx {
    Project { project_id: Uuid },
    Story { project_id: Uuid, story_id: Uuid },
    Task { project_id: Uuid, story_id: Uuid, task_id: Uuid },
}
```

### 3. Contracts

#### Response — StoryRunsResponse

| Field | Type | Constraints |
|-------|------|-------------|
| story_id | String | UUID |
| runs | Vec\<StoryRunOverviewDto\> | Sorted by created_at desc |

#### StoryRunOverviewDto

| Field | Type | Constraints |
|-------|------|-------------|
| id | String | Run UUID |
| lifecycle_id | String | UUID |
| status | LifecycleRunStatus | Running/Ready/Blocked/Completed/Failed/Cancelled |
| session_id | Option\<String\> | Active agent session |
| created_at | String | ISO 8601 |
| updated_at | String | ISO 8601 |
| last_activity_at | String | ISO 8601 |
| links | Vec\<LifecycleRunLinkDto\> | This run's links |

#### LifecycleRunLinkDto

| Field | Type | Constraints |
|-------|------|-------------|
| id | String | UUID |
| run_id | String | UUID |
| subject_kind | String | snake_case enum |
| subject_id | String | UUID |
| role | String | snake_case enum |
| metadata | Option\<Value\> | JSON |
| created_at | String | ISO 8601 |

#### CapabilityScope 推导规则

```
session_id → lifecycle_run_repo.find_by_session(session_id) → run
           → lifecycle_run_link_repo.list_by_run(run_id) → links
           → if links contain Task subject → Task scope
           → else if links contain Story subject → Story scope
           → else → Project scope
```

### 4. Validation & Error Matrix

| Condition | Error |
|-----------|-------|
| story_id 格式无效 | `400 BadRequest("无效的 story_id: {v}")` |
| story 不存在或无权访问 | `404 NotFound` / `403 Forbidden` |
| run_id 无效 UUID | `400 BadRequest` |
| cascade delete: lifecycle_run 删除 | `lifecycle_run_links` 自动级联删除 |
| 重复 link（same run_id + subject + role） | 当前不做 unique constraint，允许多条（幂等由 application 保证） |

### 5. Good / Base / Bad Cases

#### Good: Story 的 Runs 查询

```
GET /stories/{story_id}/runs

→ list_by_subject(Story, story_id) → links
→ extract run_ids → list_by_ids(run_ids)
→ 返回 StoryRunsResponse with 完整 link 信息
```

#### Base: Session 的业务上下文推导

```
session_id → find_by_session → run
→ list_by_run(run_id) → links
→ links 中有 Story(Subject) + Task(ProjectionTarget)
→ 推导 CapabilityScopeCtx::Task { project_id, story_id, task_id }
```

#### Bad: SessionBinding 遗留路径（已删除）

```
// 旧路径：session_id → session_bindings → owner_type/owner_id
// 已不可用。migration 0070 已 DROP TABLE session_bindings。
// 所有查询必须通过 LifecycleRunLink。
```

### 6. Tests Required

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `LifecycleRunLink::new` | Fields set correctly, id generated |
| Unit | `RunLinkSubjectKind::from_str` / `as_str` roundtrip | All 6 variants |
| Unit | `RunLinkRole::from_str` / `as_str` roundtrip | All 5 variants |
| Unit | `LifecycleRunLink` serde roundtrip | JSON serialize/deserialize preserves all fields |
| Unit | `LifecycleRunLinkService::active_run_for_story` | Returns Running/Ready run, ignores Completed |
| Integration | `PostgresLifecycleRunLinkRepository::create` + `list_by_subject` | Roundtrip with correct filtering |
| Integration | `list_by_subject_and_role` filters by role | Only matching role returned |
| Integration | CASCADE delete on lifecycle_runs deletion | Links auto-removed |
| API | `GET /stories/{id}/runs` with valid story | Returns runs sorted desc |
| API | `GET /stories/{id}/runs` with no links | Returns empty runs array |
| API | `GET /stories/{id}/runs/active` | Returns single active or null |

### 7. Wrong vs Correct

#### Wrong: Looking up Story by Session (Old SessionBinding Path)

```rust
// WRONG — session_bindings table no longer exists
let bindings = session_binding_repo.list_by_session(session_id).await?;
let story_id = bindings.iter()
    .find(|b| b.owner_type == "story")
    .map(|b| b.owner_id);
```

#### Correct: Session → Run → Links → Subject

```rust
// CORRECT — go through LifecycleRunLink
let run = lifecycle_run_repo.find_by_session(session_id).await?;
if let Some(run) = run {
    let links = lifecycle_run_link_repo.list_by_run(run.id).await?;
    let story_id = links.iter()
        .find(|l| l.subject_kind == RunLinkSubjectKind::Story && l.role == RunLinkRole::Subject)
        .map(|l| l.subject_id);
}
```

---

## Design Decision: LifecycleRunLink vs Embedded Foreign Keys

**Context**: LifecycleRun 需要与多种业务对象建立关系（Story, Task, RoutineExecution, Project, 另一个 Run）。

**Options**:
1. 为每种对象类型在 `lifecycle_runs` 表上加 nullable FK 列
2. 独立 join table with (run_id, subject_kind, subject_id, role) 四元组

**Decision**: Option 2。原因：
- 避免 `lifecycle_runs` 表列数膨胀
- 一个 run 可同时关联多个对象（如：Source=RoutineExecution + Subject=Story + ProjectionTarget=Task）
- role 语义让查询方向明确（"这个 run 正在处理哪个 Story" vs "哪些 run 是被这个 routine 触发的"）
- 新增 subject 类型只需扩展 enum，不需要 schema migration

**Extensibility**: 新增 `RunLinkSubjectKind` 变体 + `RunLinkRole` 变体即可表达新关系类型。

---

## Design Decision: Session Demotion

**Context**: `Session` 原来通过 `session_bindings` 表承载了 ownership 语义（"这个 session 属于哪个 Story"），导致 session 概念混合了 runtime container 和 business ownership。

**Decision**: 将 Session 降级为纯 runtime event stream 容器：
- 删除 `session_bindings` 表
- 在 `sessions` 表上新增 `project_id` 列（仅用于按项目查询 session 列表）
- 所有 business ownership 查询通过 `LifecycleRun.session_id → LifecycleRunLink` 路径

这使得 Session 可以独立于业务语义存在（例如 debug session、sandbox session）。
