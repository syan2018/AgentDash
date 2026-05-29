# Permission Policy Engine Contract

> 评估 Agent 权限申请是否可自动批准的纯函数策略引擎。

---

## Scenario: Policy Evaluation & Scope Escalation

### 1. Scope / Trigger

- Agent 提交 capability grant request 后，必须经 policy engine 评估才能进入 approval 流程
- Scope escalation 在 action 实际成功后由 coordinator 验证 intent 并创建关联层
- 涉及跨模块数据流：`ProjectAgent.config` + `WorkflowContract` → `PolicyDecision` → grant state transition

### 2. Signatures

#### Policy Service

```rust
// crates/agentdash-application/src/permission/policy.rs
pub struct PermissionPolicyService;

impl PermissionPolicyService {
    pub fn evaluate(
        requested_paths: &[ToolCapabilityPath],
        agent_auto_grantable: &[ToolCapabilityPath],
        lifecycle_requestable: &[ToolCapabilityPath],
    ) -> PolicyDecision;

    pub fn extract_agent_grantable(config: &serde_json::Value) -> Vec<ToolCapabilityPath>;
    pub fn extract_lifecycle_requestable(contract: &serde_json::Value) -> Vec<ToolCapabilityPath>;
}
```

#### Grant Compiler

```rust
// crates/agentdash-application/src/permission/compiler.rs
pub struct PermissionGrantCompiler;

impl PermissionGrantCompiler {
    pub fn compile(grant: &PermissionGrant) -> RuntimeCapabilityTransition;
}
```

#### Scope Escalation Coordinator

```rust
// crates/agentdash-application/src/permission/escalation.rs
pub struct ScopeEscalationCoordinator {
    grant_repo: Arc<dyn PermissionGrantRepository>,
    link_repo: Arc<dyn LifecycleRunLinkRepository>,
}

impl ScopeEscalationCoordinator {
    pub async fn try_escalate(
        &self,
        session_id: &str,
        created_subject_kind: RunLinkSubjectKind,
        created_subject_id: Uuid,
    ) -> Result<Option<EscalationResult>, String>;
}

pub struct EscalationResult {
    pub grant_id: Uuid,
    pub link: LifecycleRunLink,
    pub unlocked_paths: Vec<ToolCapabilityPath>,
}
```

### 3. Contracts

#### Policy Evaluation Input

| Parameter | Source | Type | Constraints |
|-----------|--------|------|-------------|
| requested_paths | companion_request payload | `Vec<ToolCapabilityPath>` | Non-empty（empty → Rejected） |
| agent_auto_grantable | `ProjectAgent.config.auto_grantable_capabilities` | `Vec<ToolCapabilityPath>` | From JSON array of strings |
| lifecycle_requestable | `WorkflowContract.requestable_capabilities` | `Vec<ToolCapabilityPath>` | From JSON array of strings |

#### Policy Evaluation Output

```rust
pub struct PolicyDecision {
    pub outcome: PolicyOutcome,       // AutoApproved / NeedsUserApproval / Rejected
    pub matched_rules: Vec<String>,   // Paths that matched auto-approve pool
    pub reason: String,               // Human-readable explanation
}
```

#### ToolCapabilityPath Coverage Rules

| covering | target | Result |
|----------|--------|--------|
| `*` (wildcard) | anything | covers |
| `story_management` (cap-level) | `story_management::create_story` | covers |
| `story_management::*` (tool-wildcard) | `story_management::create_story` | covers |
| `story_management::create_story` | `story_management::create_story` | covers (exact) |
| `story_management::create_story` | `story_management` | does NOT cover |
| `task_management` | `story_management` | does NOT cover |

#### Compiler Output

`RuntimeCapabilityTransition` with:
- `declarations`: One `CapabilityDeclarationRecord` per requested path
  - `dimension`: `CapabilityDimensionKey("tool")`
  - `declaration_type`: `"capability_directive"`
  - `source`: `CapabilityArtifactSource::permission_grant()`
  - `payload`: `ToolCapabilityDirective::Add(path)` serialized as JSON
- `effects`: empty vec

#### Scope Escalation Input/Output

| Input | Type | Constraint |
|-------|------|-----------|
| session_id | &str | Active session with applied grant |
| created_subject_kind | RunLinkSubjectKind | Must match grant's `scope_escalation_intent.target_subject_kind` |
| created_subject_id | Uuid | The just-created entity ID |

Result if matched: Creates `LifecycleRunLink(role=ControlScope)` + marks grant `ScopeEscalated` + returns `unlocked_paths`.

### 4. Validation & Error Matrix

| Condition | Result |
|-----------|--------|
| `requested_paths` is empty | `PolicyOutcome::Rejected` |
| `agent_auto_grantable` is empty | All paths → `NeedsUserApproval` (no auto-approve pool) |
| `lifecycle_requestable` is empty | All paths → `NeedsUserApproval` (no auto-approve pool) |
| All paths in auto-approve pool | `AutoApproved` |
| Some paths in pool, some not | `NeedsUserApproval` (conservative: partial match → user decides) |
| No paths in pool | `NeedsUserApproval` |
| Escalation: no matching grant | Returns `Ok(None)` — no-op |
| Escalation: intent kind mismatch | Returns `Ok(None)` — no-op |
| Escalation: link creation fails | Returns `Err` with context |

### 5. Good / Base / Bad Cases

#### Good: Patrol Agent Auto-Approve

```
agent_config.auto_grantable_capabilities: ["story_management", "task_management"]
lifecycle_contract.requestable_capabilities: ["story_management", "task_management"]
requested_paths: ["story_management::create_story"]

→ auto_approve_pool = [story_management, task_management]
→ story_management covers story_management::create_story
→ PolicyOutcome::AutoApproved
```

#### Base: Scope Escalation Post-Create

```
Grant applied with scope_escalation_intent:
  { target_subject_kind: Story, unlocked_paths: [task_management] }

Agent calls create_story → success → story_id = X
→ ScopeEscalationCoordinator::try_escalate(session, Story, X)
→ Creates LifecycleRunLink(run_id, Story, X, ControlScope)
→ Grant → ScopeEscalated
→ Returns unlocked_paths: [task_management]
→ Caller compiles secondary RuntimeCapabilityTransition
```

#### Bad: Partial Coverage Conservative Reject

```
agent_auto_grantable: ["story_management"]
lifecycle_requestable: ["story_management", "admin"]
requested_paths: ["story_management", "admin"]

→ auto_approve_pool = [story_management] (only agent's story_management covered by lifecycle)
→ story_management: matched; admin: unmatched
→ PolicyOutcome::NeedsUserApproval (partial → conservative)
```

### 6. Tests Required

| Level | Target | Assertion |
|-------|--------|-----------|
| Unit | `evaluate` with all covered | `AutoApproved`, matched_rules non-empty |
| Unit | `evaluate` with empty requested_paths | `Rejected` |
| Unit | `evaluate` with empty agent_auto_grantable | `NeedsUserApproval` |
| Unit | `evaluate` partial coverage | `NeedsUserApproval`, matched_rules partial |
| Unit | `path_covers` capability-level → tool-level | Returns true |
| Unit | `path_covers` wildcard `*` | Covers everything |
| Unit | `path_covers` tool-specific → capability-level | Returns false |
| Unit | `extract_agent_grantable` from valid JSON | Correct Vec |
| Unit | `extract_agent_grantable` from missing key | Empty Vec |
| Unit | `PermissionGrantCompiler::compile` | Correct dimension, type, source, payload |
| Unit | `PermissionGrantCompiler::compile` payload deserializes to `Add` | Validates roundtrip |
| Integration | `ScopeEscalationCoordinator::try_escalate` with matching grant | Link created, grant status = ScopeEscalated |
| Integration | `try_escalate` with no matching grant | Returns `Ok(None)` |

### 7. Wrong vs Correct

#### Wrong: Checking coverage without intersection

```rust
// WRONG — agent declarations alone don't determine auto-approve
fn evaluate(requested: &[Path], agent_grantable: &[Path]) -> PolicyDecision {
    if requested.iter().all(|p| agent_grantable.contains(p)) {
        PolicyDecision { outcome: AutoApproved, .. }
    }
}
```

#### Correct: Intersection of agent AND lifecycle declarations

```rust
// CORRECT — both sources must agree
let auto_approve_pool = compute_auto_approve_pool(agent_auto_grantable, lifecycle_requestable);
// Only paths in the intersection get auto-approved
```

---

## Design Decision: Conservative Partial-Match Strategy

**Context**: When some requested paths are auto-approvable but others are not.

**Options**:
1. Split: auto-approve covered paths, ask user for uncovered
2. Conservative: require user approval for all if any uncovered

**Decision**: Option 2 (conservative). Rationale: partial auto-grant could leave Agent in a confusing half-capable state. Better UX for user to see the complete request and approve/reject atomically.

**Extensibility**: Could add a `split_grant_on_partial` option to `PolicyService::evaluate` if UX research shows users prefer granular approval.

---

## Design Decision: CapabilityArtifactSource for Grants

**Context**: Need to identify capability declarations originating from permission grants (vs static config, plugins, etc.) for auditability.

**Decision**: Added `CapabilityArtifactSource::permission_grant()` constructor:

```rust
// crates/agentdash-spi/src/session_persistence.rs
impl CapabilityArtifactSource {
    pub fn permission_grant() -> Self {
        Self { kind: "permission_grant".to_string() }
    }
}
```

This allows capability replay/projection to attribute declarations to their origin for debugging and revocation.
