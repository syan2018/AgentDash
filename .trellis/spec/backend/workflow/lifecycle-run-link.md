# LifecycleSubjectAssociation Contract

> 本 appendix 定义目标关联层。当前 `LifecycleRunLink` 是迁移来源；目标实体是 `LifecycleSubjectAssociation`，用于表达 `SubjectRef` 到 whole run 或 `LifecycleAgent` 的关系。

---

## Scope

- `RuntimeSession` 降级为 runtime trace container，不承载 business ownership。
- `LifecycleRun` 是 tracked life process / control ledger，不直接拥有 subject aggregate body。
- `LifecycleSubjectAssociation` 统一表达 source、subject、projection、control scope、lineage 等关系。
- Runtime node 不作为 subject anchor；执行证据来自 `RuntimeSessionExecutionAnchor`、orchestration journal、artifact 与 event。

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
  -> optional OrchestrationInstance / RuntimeNodeState by orchestration_id + node_path + attempt
```

RuntimeSessionExecutionAnchor 是 runtime trace 到 run / agent / frame / orchestration node 的权威索引，原因是 `RuntimeSession` 只表达消息流和投递证据，业务归属必须落到 lifecycle 控制面。

### Task Projection

```text
SubjectRef(kind=Task, id=task_id)
  -> agent-scoped association
  -> LifecycleRun.orchestrations[]
  -> RuntimeNodeState
  -> artifacts
  -> SubjectExecutionView.task_projection
```

### Project AgentRun 列表收束（主 Run vs subagent）

面向用户的 AgentRun 列表（`GET /projects/:id/agent-runs`）按 **AgentLineage 控制树** 收束，每个 Run 只产出"主 Run"：

```text
project_id
  -> lifecycle_run_repo.list_by_project
  -> 每个 run: lifecycle_agent_repo.list_by_run + agent_lineage_repo.list_children 建内存 forest
  -> root = 从未作为 lineage child 出现的 agent（主 Run）
  -> subagent_count = root 子树传递闭包后代数（DFS + visited 防环 + 深度上限）
```

约定（contract）：

- **主/从关系的真值源是 `AgentLineage`，不是 `LifecycleAgent.agent_role`**。所有创建路径（含被派发的子 agent）都经 `LifecycleAgent::new_root`，历史上 `agent_role` 恒为 `primary`；现按 `agent_role::{PRIMARY, SUBAGENT, COMPANION}` 在创建时写入真实值，但它**仅作冗余快捷标记**用于展示/过滤，收束与嵌套判定一律回到 lineage。
- lineage 控制树**支持任意深度递归且无环检测**（subagent 可再派发 subagent，`parent_agent_id = 派发者 anchor.agent_id`）。任何后代遍历（后端 `count_descendants` / 前端递归展开）必须带 `visited` 防环 + 深度上限保护，超限 `warn` 而非静默。
- `AgentRunWorkspaceView.parent` / `.children` 提供一跳 lineage 引用（`AgentRunLineageRef`），供右侧会话栏展示从属与跳转；列表 entry 只带 `subagent_count`，不内联 children，避免 N×M 膨胀。

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
