# RuntimeSession Frame / Assignment Anchor Design

## Target Contract

新增或收敛一个应用层 anchor read model：

```rust
pub struct RuntimeSessionFrameAnchor {
    pub runtime_session_id: String,
    pub frame_id: Uuid,
    pub agent_id: Uuid,
    pub run_id: Uuid,
}

pub struct RuntimeSessionActivityAnchor {
    pub runtime_session: RuntimeSessionFrameAnchor,
    pub assignment_id: Uuid,
    pub graph_instance_id: Uuid,
    pub activity_key: String,
    pub attempt: u32,
    pub turn_id: Option<String>,
    pub active: bool,
}
```

推荐优先引入 `RuntimeSessionExecutionAnchorRepository` 或等价实体，使 Activity terminal 主路径不依赖 `runtime_session_refs_json`。若短期只做 service 组合查询，也必须保证查询路径不再扫描 run 下全部 assignments 并 fallback。

## Query Rule

```text
runtime_session_id
  -> runtime_session_execution_anchor
  -> AgentAssignment by assignment_id
  -> LifecycleRun by assignment.run_id
```

允许无 activity assignment 的 frame session 返回 frame anchor；activity terminal / activity advance 必须要求 activity anchor。

ContinueRoot / reused runtime session 可以让同一 runtime session 顺序承接多个 assignment。该场景必须通过 `turn_id` 或 active anchor 状态唯一定位当前 assignment；不能通过 frame 当前 graph/activity 或 agent 下唯一 active assignment 推导。

## Rejection Rule

- runtime session 不属于任何 frame：not found。
- runtime session 同时命中多个 delivery frame 且无法按 current delivery policy 唯一选择：conflict。
- frame 没有 active assignment：activity anchor absent。
- frame 有多个 active assignment：conflict。
- assignment attempt 不能转成 `u32`：invalid domain state。

## API / DTO Shape

可以在 contracts 中增加：

```rust
pub struct RuntimeSessionFrameAnchorDto {
    pub runtime_session_ref: RuntimeSessionRefDto,
    pub frame_ref: AgentFrameRefDto,
    pub agent_ref: LifecycleAgentRefDto,
    pub run_ref: LifecycleRunRefDto,
}

pub struct RuntimeSessionActivityAnchorDto {
    pub runtime_session_anchor: RuntimeSessionFrameAnchorDto,
    pub assignment_ref: AgentAssignmentRefDto,
    pub attempt_ref: ActivityAttemptRefDto,
    pub graph_instance_id: String,
    pub activity_key: String,
    pub attempt: u32,
    pub delivery_role: String,
}
```

## Affected Areas

- `crates/agentdash-application/src/workflow/session_association.rs`
- `crates/agentdash-application/src/workflow/orchestrator.rs`
- `crates/agentdash-domain/src/workflow/repository.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`
- `crates/agentdash-api/src/routes/lifecycle_views.rs`
- `crates/agentdash-contracts/src/workflow.rs`

## Validation

- Unit: select logic only accepts assignment by exact `frame_id`。
- Integration: terminal callback can advance an activity attempt through direct anchor。
- Integration: freeform runtime session returns frame anchor and no activity anchor。
- Integration: duplicate active assignment for one frame returns conflict。
