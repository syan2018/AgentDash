# 目标锚点 Schema 设计

## 目标

本任务只建立控制面目标事实源，不切业务入口。完成后，后续任务可以通过 repository 明确创建或查询 `WorkflowGraphInstance`、`LifecycleAgent`、`AgentFrame`、`AgentAssignment`、`LifecycleSubjectAssociation`、`LifecycleGate`、`AgentLineage`，而不是继续把 `LifecycleRun.session_id`、`LifecycleRun.lifecycle_id`、`LifecycleRunLink` 或 `ExecutorRunRef::AgentSession` 当作主路径。

## 实体合同

| 表 / 实体 | 最小字段 | 关键约束 | 首个写入来源 |
| --- | --- | --- | --- |
| `lifecycle_workflow_instances` | `id`、`run_id`、`graph_id`、`role`、`status`、`activity_state_json`、`created_at`、`updated_at` | root graph 以 `(run_id, role=root)` 唯一；activity state 后续必须以 `graph_instance_id` 命名空间读写 | `LifecycleRun.lifecycle_id` backfill；dispatch |
| `lifecycle_agents` | `id`、`run_id`、`project_id`、`agent_kind`、`agent_role`、`project_agent_id?`、`status`、`current_frame_id?`、`created_at`、`updated_at` | agent 只属于一个 run；`current_frame_id` 必须指向同 agent 的 frame | `LifecycleRun.session_id` backfill；dispatch |
| `agent_frames` | `id`、`agent_id`、`revision`、`procedure_id?`、`graph_instance_id?`、`activity_key?`、`effective_capability_json`、`context_slice_json`、`vfs_surface_json`、`mcp_surface_json`、`runtime_session_refs_json`、`created_by_kind`、`created_by_id?`、`created_at` | `(agent_id, revision)` 唯一；runtime session refs 只作为 trace/delivery refs | root frame backfill；AgentFrame builder |
| `agent_assignments` | `id`、`run_id`、`graph_instance_id`、`activity_key`、`attempt`、`agent_id`、`frame_id`、`lease_status`、`assigned_at`、`released_at?` | 必须包含 `graph_instance_id + activity_key + attempt`；attempt terminal 仍归 `ActivityAttemptState` | scheduler / B4；本任务只提供 repository |
| `lifecycle_subject_associations` | `id`、`anchor_run_id`、`anchor_agent_id?`、`subject_kind`、`subject_id`、`role`、`metadata_json`、`created_at` | anchor 只能是 run 或 agent；`anchor_agent_id` 非空时必须属于 `anchor_run_id` | `LifecycleRunLink` backfill；dispatch |
| `lifecycle_gates` | `id`、`run_id`、`agent_id?`、`frame_id?`、`gate_kind`、`correlation_id`、`status`、`payload_json`、`resolved_by?`、`created_at`、`resolved_at?` | open gate 必须能恢复 run/agent/frame context；correlation 可用于 resume | companion / permission / human wait |
| `agent_lineages` | `id`、`run_id`、`parent_agent_id?`、`child_agent_id`、`relation_kind`、`source_frame_id?`、`metadata_json`、`created_at` | Agent control tree 使用本表；`RuntimeSessionLineage` 只保留 trace/debug | companion / spawn policy |

## Repository 合同

- `WorkflowGraphInstanceRepository`
  - `create_root_from_run_lifecycle(run_id, lifecycle_id)`
  - `list_by_run(run_id)`
  - `get(run_id, graph_instance_id)`
- `LifecycleAgentRepository`
  - `create_root_agent_for_run(run_id, project_id, runtime_session_ref?)`
  - `list_by_run(run_id)`
  - `find_by_runtime_session(runtime_session_id)`，实现上通过 frame refs 查询，语义上是 trace 反查。
- `AgentFrameRepository`
  - `append_revision(agent_id, frame_delta)`
  - `get_current(agent_id)`
  - `find_by_runtime_session(runtime_session_id)`
- `AgentAssignmentRepository`
  - `assign(run_id, graph_instance_id, activity_key, attempt, agent_id, frame_id)`
  - `find_for_attempt(graph_instance_id, activity_key, attempt)`
  - `find_by_runtime_session(runtime_session_id)` 只用于 terminal trace lookup。
- `LifecycleSubjectAssociationRepository`
  - `link_run_subject(run_id, subject_ref, role, metadata)`
  - `link_agent_subject(agent_id, subject_ref, role, metadata)`
  - `list_by_subject(subject_ref)`
  - `list_by_anchor(run_id, agent_id?)`
- `LifecycleGateRepository`
  - `open_gate(run_id, agent_id?, frame_id?, gate_kind, correlation_id, payload)`
  - `resolve_gate(gate_id, resolved_by, payload)`
  - `list_open_for_agent(agent_id)`
- `AgentLineageRepository`
  - `record(parent_agent_id?, child_agent_id, relation_kind, source_frame_id?, metadata)`
  - `list_children(agent_id)`
  - `find_parent(agent_id)`

## Backfill 边界

本任务只做结构性 backfill：

- `LifecycleRun.lifecycle_id` -> `WorkflowGraphInstance(role=root)`。
- `LifecycleRun.session_id` -> root `LifecycleAgent` + root `AgentFrame.runtime_session_refs`。
- `LifecycleRunLink` -> `LifecycleSubjectAssociation(anchor_run_id=run_id, anchor_agent_id=null)`。
- `ExecutorRunRef::AgentSession` 可以被记录为 frame runtime evidence，但 attempt 到 assignment 的完整语义由 `workflow-agent-assignment-migration` 接管。
- `session_lineage` 只在能确定 parent/child agent 时 backfill 到 `agent_lineages`；否则保持 `RuntimeSessionLineage` trace。

## 不变量

- `RuntimeSession` 不拥有业务 owner；通过 runtime session 查 run 只能走 frame/agent trace lookup。
- `LifecycleRun` 不再承诺单 graph；`lifecycle_id` 只是 root graph backfill 来源。
- Activity state、claim、assignment、attempt 的目标 key 都必须包含 `graph_instance_id`。
- `LifecycleSubjectAssociation` 不允许 Activity / Attempt anchor。
- `AgentFrame` 的 runtime refs 是 delivery/trace refs，不是 subject association。

## 断裂点

本任务落地后，旧业务入口可能仍然只写 `LifecycleRun.session_id` 或 `ExecutorRunRef::AgentSession`。允许系统处于“新表可读但未被主路径使用”的状态；恢复主路径由 `lifecycle-dispatch-service`、`agent-frame-construction-migration`、`workflow-agent-assignment-migration` 接续。
