# Permission Grant Lifecycle Contract

> PermissionGrant 聚合根的创建、状态机转换、持久化、REST API 完整契约。

---

## Scenario: Agent Capability Grant Request & Approval

### 1. Scope / Trigger

- Agent runtime 或 platform broker 通过 `PermissionGrantService::request` 发起 capability 申请
- `permission_grants` 数据库表 + REST endpoints
- 跨层数据流：domain entity → application service → infrastructure repo → API handler → frontend card
- Companion `capability_grant_request` 只作为交互/broker 输入；授权事实以 `PermissionGrant` 聚合为准。

### 2. Signatures

#### Domain Entity

```rust
// crates/agentdash-domain/src/permission/entity.rs
pub struct PermissionGrant {
    pub id: Uuid,
    pub run_id: Uuid,
    pub effect_frame_id: Option<Uuid>,
    pub source_runtime_session_id: String,
    pub source_turn_id: Option<String>,
    pub source_tool_call_id: Option<String>,
    pub requested_paths: Vec<ToolCapabilityPath>,
    pub reason: String,
    pub grant_scope: GrantScope,
    pub expires_at: Option<DateTime<Utc>>,
    pub scope_escalation_intent: Option<ScopeEscalationIntent>,
    pub status: GrantStatus,
    pub policy_decision: Option<PolicyDecision>,
    pub approved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### Repository Trait

```rust
// crates/agentdash-domain/src/permission/repository.rs
#[async_trait]
pub trait PermissionGrantRepository: Send + Sync {
    async fn create(&self, grant: &PermissionGrant) -> Result<(), DomainError>;
    async fn update(&self, grant: &PermissionGrant) -> Result<(), DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<PermissionGrant>, DomainError>;
    async fn list_by_frame(&self, effect_frame_id: Uuid, status_filter: Option<PermissionGrantStatusFilter>) -> Result<Vec<PermissionGrant>, DomainError>;
    async fn list_by_run(&self, run_id: Uuid, status_filter: Option<PermissionGrantStatusFilter>) -> Result<Vec<PermissionGrant>, DomainError>;
    async fn list_active_by_frame(&self, effect_frame_id: Uuid) -> Result<Vec<PermissionGrant>, DomainError>;
    async fn list_active_by_run(&self, run_id: Uuid) -> Result<Vec<PermissionGrant>, DomainError>;
    async fn find_active_escalation_grant(&self, session_id: &str, target_subject_kind: &str) -> Result<Option<PermissionGrant>, DomainError>;
    async fn expire_overdue(&self) -> Result<u64, DomainError>;
}
```

#### Database Schema

```sql
-- migrations/0071_permission_grants.sql
CREATE TABLE IF NOT EXISTS permission_grants (
    id TEXT PRIMARY KEY,               -- UUID as text
    run_id TEXT NOT NULL,              -- FK to lifecycle_runs (logical)
    effect_frame_id TEXT,
    source_runtime_session_id TEXT NOT NULL,
    source_turn_id TEXT,
    source_tool_call_id TEXT,
    requested_paths TEXT NOT NULL,     -- JSON array: ["story_management", "task_management::execution_view"]
    reason TEXT NOT NULL,
    grant_scope TEXT NOT NULL,         -- enum: turn / agent_frame / activity
    expires_at TEXT,                   -- ISO 8601
    scope_escalation_intent TEXT,      -- JSON object or NULL
    status TEXT NOT NULL DEFAULT 'created',
    policy_decision TEXT,              -- JSON object or NULL
    approved_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Partial indexes for active grant queries
CREATE INDEX idx_permission_grants_frame_active
    ON permission_grants(effect_frame_id)
    WHERE status IN ('applied', 'scope_escalated');

CREATE INDEX idx_permission_grants_run
    ON permission_grants(run_id);

CREATE INDEX idx_permission_grants_status
    ON permission_grants(status)
    WHERE status IN ('applied', 'scope_escalated', 'pending_user_approval');
```

#### REST API

| Method | Path | Handler | Auth |
|--------|------|---------|------|
| GET | `/permission-grants?effect_frame_id=&run_id=&status=&status_group=` | `list_grants` | CurrentUser |
| GET | `/permission-grants/:id` | `get_grant` | CurrentUser |
| POST | `/permission-grants/:id/approve` | `approve_grant` | CurrentUser |
| POST | `/permission-grants/:id/reject` | `reject_grant` | CurrentUser |
| POST | `/permission-grants/:id/revoke` | `revoke_grant` | CurrentUser |

### 3. Contracts

#### Request — List Grants

| Field | Type | Required | Constraints |
|-------|------|----------|-------------|
| effect_frame_id | String (query) | One of effect_frame_id/run_id required | Valid UUID |
| run_id | String (query) | One of effect_frame_id/run_id required | Valid UUID |
| status | String (query) | Optional | Exact grant status |
| status_group | String (query) | Optional | `pending` / `active` / `terminal`; mutually exclusive with `status` |

#### Response — PermissionGrantDto

| Field | Type | Constraints |
|-------|------|-------------|
| id | String | UUID |
| run_id | String | UUID |
| effect_frame_id | Option\<String\> | AgentFrame UUID when grant is frame-scoped |
| source_runtime_session_id | String | Runtime trace audit ref |
| requested_paths | Vec\<String\> | e.g. `["story_management", "task_management::execution_view"]` |
| reason | String | Agent-provided justification |
| grant_scope | String | `turn` / `agent_frame` / `activity` |
| expires_at | Option\<String\> | ISO 8601 |
| scope_escalation_intent | Option\<ScopeEscalationIntentDto\> | typed target scope + unlocked paths |
| status | String | GrantStatus snake_case |
| policy_decision | Option\<PolicyDecisionDto\> | typed policy outcome, matched rules and reason |
| approved_by | Option\<String\> | user_id or "system" |
| created_at | String | ISO 8601 |
| updated_at | String | ISO 8601 |

### 4. Validation & Error Matrix

| Condition | Error |
|-----------|-------|
| list: neither effect_frame_id nor run_id | `400 BadRequest("effect_frame_id or run_id query param required")` |
| list: both status and status_group | `400 BadRequest("status and status_group cannot be used together")` |
| list: invalid effect_frame_id UUID | `400 BadRequest("invalid effect_frame_id: {v}")` |
| list: invalid run_id UUID | `400 BadRequest("invalid run_id: {v}")` |
| get/approve/reject/revoke: invalid grant_id | `400 BadRequest("invalid grant_id: {v}")` |
| get/approve/reject/revoke: grant not found | `404 NotFound("grant not found: {id}")` |
| approve: status != PendingUserApproval | `400 BadRequest("grant is not pending user approval")` |
| reject: status != PendingUserApproval | `400 BadRequest("grant is not pending user approval")` |
| revoke: !status.is_active() | `400 BadRequest("grant is not active")` |
| domain state transition invalid | `500 Internal("state transition failed: {err}")` |

### 5. Good / Base / Bad Cases

#### Good: Auto-Approved Flow

```
Agent requests story_management
→ PolicyService: agent_auto_grantable ∩ lifecycle_requestable 命中
→ status: Created → PendingPolicy → Approved → Applied
→ PermissionGrantCompiler → RuntimeCapabilityTransition applied
→ Agent 获得 story tools
```

#### Base: User Approval Flow

```
Agent requests workflow_management（not in auto-approve pool）
→ PolicyService: NeedsUserApproval
→ status: Created → PendingPolicy → PendingUserApproval
→ User clicks "Approve" in PermissionGrantCard
→ POST /permission-grants/:id/approve → status: Approved → Applied
```

#### Bad: Rejected at Policy

```
Agent requests unknown_capability
→ PolicyService: 不在 lifecycle_requestable 范围
→ status: Created → PendingPolicy → Rejected (terminal)
→ Agent 收到拒绝响应
```

### 6. Tests Required

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `PermissionGrant::submit_for_policy` on non-Created status | Returns `DomainError::InvalidTransition` |
| Unit | `PermissionGrant` happy path (auto-approve → applied) | Status transitions correctly, `approved_by = "system"` |
| Unit | `PermissionGrant` user approve path | `approved_by` set to user_id |
| Unit | `PermissionGrant` scope escalation path | Status reaches `ScopeEscalated` |
| Unit | `GrantStatus::is_active()` / `is_terminal()` | Applied/ScopeEscalated active; Rejected/Expired/Revoked terminal |
| Unit | `GrantScope::from_str` / `as_str` roundtrip | All variants |
| Integration | `PostgresPermissionGrantRepository::create` + `find_by_id` | Roundtrip preserves all fields |
| Integration | `list_by_frame(..., Active)` filters non-active | Only Applied/ScopeEscalated returned |
| Integration | `list_by_run(..., Pending/Terminal)` filters by status group | Pending and terminal groups do not rely on active-only query |
| Integration | `expire_overdue` marks old grants | Applied + expired → Expired |
| API | `GET /permission-grants` without params | 400 error |
| API | `POST /permission-grants/:id/approve` on Applied grant | 400 error |

### 7. Wrong vs Correct

#### Wrong: Skipping State Machine in API Handler

```rust
// WRONG — directly setting status bypasses domain validation
grant.status = GrantStatus::Applied;
state.repos.permission_grant_repo.update(&grant).await?;
```

#### Correct: Using Domain Methods

```rust
// CORRECT — state transition enforced by entity method
grant.user_approve(&current_user.user_id)
    .map_err(|e| ApiError::Internal(format!("state transition failed: {e}")))?;
grant.mark_applied()
    .map_err(|e| ApiError::Internal(format!("mark_applied failed: {e}")))?;
state.repos.permission_grant_repo.update(&grant).await?;
```

---

## State Machine Diagram

```
Created ──submit_for_policy──→ PendingPolicy
                                     │
                    ┌────────────────┼────────────────┐
                    ↓                ↓                ↓
              AutoApproved     NeedsUser          Rejected (terminal)
                    │                │
                    ↓                ↓
               Approved ←── user_approve ─── PendingUserApproval
                    │                              │
              ┌─────┤                        user_reject
              ↓     ↓                              ↓
          Applied  Failed (terminal)          Rejected (terminal)
              │
    ┌─────┬──┴──┬────────┐
    ↓     ↓     ↓        ↓
Expired Revoked ScopeEscalated
 (term)  (term)    (active)
```

Active states: `Applied`, `ScopeEscalated`
Terminal states: `Rejected`, `Failed`, `Expired`, `Revoked`
