# LifecycleSubjectAssociation Contract

> 本 appendix 定义目标关联层。当前 `LifecycleRunLink` 是迁移来源；目标实体是 `LifecycleSubjectAssociation`，用于表达 `SubjectRef` 到 whole run 或 `LifecycleAgent` 的关系。

---

## Scope

- `RuntimeSession` 降级为 runtime trace container，不承载 business ownership。
- `LifecycleRun` 是 tracked life process / control ledger，不直接拥有 subject aggregate body。
- `LifecycleSubjectAssociation` 统一表达 source、subject、projection、control scope、lineage 等关系。
- Activity / ActivityAttemptState 不作为 subject anchor；执行证据来自 `AgentAssignment`、artifact 与 event。

## Domain Entity

```rust
pub struct LifecycleSubjectAssociation {
    pub id: Uuid,
    pub anchor_run_id: Uuid,
    pub anchor_agent_id: Option<Uuid>,
    pub subject_kind: SubjectKind,
    pub subject_id: Uuid,
    pub role: SubjectAssociationRole,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

pub enum SubjectKind {
    Project,
    Story,
    Task,
    RoutineExecution,
    LifecycleRun,
    External,
}

pub enum SubjectAssociationRole {
    Source,
    Subject,
    ProjectionTarget,
    ControlScope,
    Lineage,
}
```

Anchor rules:

```text
anchor_agent_id = null
  -> whole-run association

anchor_agent_id != null
  -> LifecycleAgent-scoped association
```

`anchor_agent_id` 非空时，该 agent 必须属于 `anchor_run_id`。

## Repository Trait

```rust
#[async_trait]
pub trait LifecycleSubjectAssociationRepository: Send + Sync {
    async fn create(&self, association: &LifecycleSubjectAssociation) -> Result<(), DomainError>;
    async fn list_by_anchor(&self, run_id: Uuid, agent_id: Option<Uuid>) -> Result<Vec<LifecycleSubjectAssociation>, DomainError>;
    async fn list_by_subject(&self, kind: SubjectKind, id: Uuid) -> Result<Vec<LifecycleSubjectAssociation>, DomainError>;
    async fn list_by_subject_and_role(&self, kind: SubjectKind, id: Uuid, role: SubjectAssociationRole) -> Result<Vec<LifecycleSubjectAssociation>, DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    async fn delete_by_run(&self, run_id: Uuid) -> Result<(), DomainError>;
}
```

## Database Shape

```sql
CREATE TABLE IF NOT EXISTS lifecycle_subject_associations (
    id TEXT PRIMARY KEY,
    anchor_run_id TEXT NOT NULL REFERENCES lifecycle_runs(id) ON DELETE CASCADE,
    anchor_agent_id TEXT,
    subject_kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    role TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_lifecycle_subject_associations_anchor
    ON lifecycle_subject_associations(anchor_run_id, anchor_agent_id);
CREATE INDEX idx_lifecycle_subject_associations_subject
    ON lifecycle_subject_associations(subject_kind, subject_id);
CREATE INDEX idx_lifecycle_subject_associations_subject_role
    ON lifecycle_subject_associations(subject_kind, subject_id, role);
```

## Query Paths

### Subject -> Lifecycle

```text
SubjectRef(kind, id)
  -> lifecycle_subject_association_repo.list_by_subject(kind, id)
  -> anchor_run_id / anchor_agent_id
  -> LifecycleRun / LifecycleAgent
```

### RuntimeSession -> Subject（控制面反查）

```text
runtime_session_id
  -> RuntimeSessionExecutionAnchorRepository.find_by_session(runtime_session_id)
  -> launch/current AgentFrame
  -> LifecycleAgent
  -> LifecycleRun
  -> LifecycleSubjectAssociationRepository.list_by_anchor(run_id, agent_id?)
```

RuntimeSessionExecutionAnchor 是 runtime trace 到 run / agent / frame / assignment / attempt 的权威索引，原因是 `RuntimeSession` 只表达消息流和投递证据，业务归属必须落到 lifecycle 控制面。

### Task Projection

```text
SubjectRef(kind=Task, id=task_id)
  -> agent-scoped association
  -> AgentAssignment
  -> ActivityAttemptState
  -> artifacts
  -> SubjectExecutionView.task_projection
```

## DTO Contract

```rust
pub struct LifecycleSubjectAssociationDto {
    pub id: String,
    pub anchor_run_id: String,
    pub anchor_agent_id: Option<String>,
    pub subject_kind: String,
    pub subject_id: String,
    pub role: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}
```

Run / subject APIs return subject/agent/run refs and projection refs as their control-plane surface.

## Migration Sources

- 现有 `LifecycleRunLink` rows backfill 到 `LifecycleSubjectAssociation(anchor_run_id=run_id, anchor_agent_id=null, ...)`。
- `RuntimeSessionExecutionAnchor` 记录 runtime session 到 run / agent / launch frame 的索引，不进入 subject association。
- Existing `session_lineage` remains `RuntimeSessionLineage` unless parent/child agent identity can be resolved; agent control tree belongs to `AgentLineage`。

## Validation

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | Association anchor | `anchor_agent_id` 非空时属于 `anchor_run_id` |
| Unit | Subject roundtrip | kind / role serde roundtrip |
| Integration | subject lookup | `SubjectRef` 可以找到 run / agent anchors |
| Integration | runtime trace lookup | RuntimeSession 只能通过 RuntimeSessionExecutionAnchor 反查到 association |
| API | subject/run views | 不返回 binding owner shape |
