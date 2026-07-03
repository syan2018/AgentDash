# WI-00 架构 Inventory 与执行结论

## Purpose

本文件是 WI-00 的规划结果，用于把当前代码事实收束成可执行分类。后续实现不再重新讨论“是否要评估这些对象”，而是按本文件中的分类和 `decisions.md` 的正式决策推进。

范围覆盖本轮重构相关的仓储、物理表、API/DTO/frontend product identity、以及 D-019 要求的冗余表初判。

## Evidence Summary

| 证据 | 结论 |
| --- | --- |
| `LifecycleContext` 包含 `main_agent_run_id`、`agent_runs`、`frame_refs`、`permission_scope`、`budget`，但生产使用点主要是持久化 roundtrip 和默认值；实际 run/agent/frame/subject 查询来自专用表和 read model | `LifecycleRun.context` 是删除候选 |
| `LifecycleGateRepository` 有 `create/get/list_open_for_agent/update`，schema 有 run、agent、correlation、status 索引，wait/companion/workflow 多处按 open gate 查询 | `lifecycle_gates` 保留为 Lifecycle child table |
| `LifecycleSubjectAssociationRepository` 有 `list_by_subject` 和 `list_by_anchor`，schema 有 subject、anchor run、anchor agent 索引，task/routine/permission/frame construction/read model 都用 subject 反查 | `lifecycle_subject_associations` 保留为 indexed association table |
| `RuntimeSessionExecutionAnchorRepository` 当前仍是 `upsert`，并暴露 `latest_updated_anchor_for_agent`；代码已有 `DeliveryRuntimeSelectionService` 通过 current delivery 而不是 latest anchor 选择 | anchor 改为 immutable create；current delivery 独立为 AgentRun binding/state |
| `AgentFrameRepository` 暴露 `get_current`、`append_visible_canvas_mount`、`append_visible_workspace_module_ref`；`AgentFrame` 仍有多列 JSON surface 和 runtime 可变 visible refs | AgentFrame 保留 revision table，但 surface 改为 canonical typed document，visible 变更写新 revision |
| raw `/sessions/*` route 仍有 delete、fork、projection rollback、meta patch、tool approval 等写入口 | raw Session 写入口删除或降级 diagnostic；产品写入口走 AgentRun |
| `ProjectAgentRunStartResult` 顶层仍暴露 `runtime_session_id` 和 `turn_id` | 启动返回结果以 `run_ref + agent_ref + initial_message` 为产品事实，runtime ref 降级 diagnostic |
| 前端 workspace 仍从 `agentRunWorkspaceState.runtime_session_id` 构造 `WorkspaceRuntimeData.sessionId/runtimeSessionId` | 前端 product workspace 不再以 runtime session id 作为主状态 key |
| mailbox message/state 表当前 `runtime_session_id NOT NULL` 且 FK `sessions(id) ON DELETE CASCADE`，同时具备 claim、recover、order、dedup、payload cleanup | mailbox 保留 child table，但 owner 改为 AgentRun，runtime 只作 delivery ref |
| `SessionPersistence` mega trait 聚合 7 个子 store，API/bootstrap/lifecycle/runtime builder 多处以 `Arc<dyn SessionPersistence>` 注入 | 拆为窄 runtime trace ports，mega trait 不进入 application service |

## Resolved Planning Questions

| ID | 结论 | 执行工作项 |
| --- | --- | --- |
| Q-001 `LifecycleRun.context` | 删除 `LifecycleContext` 和 `lifecycle_runs.context`。其中 AgentRun/frame 索引由 `lifecycle_agents`、`agent_frames`、`agent_run_lineages`、subject association/read model 重建；`permission_scope/budget` 当前没有足够生产 consumer，若未来需要应作为明确 control-plane state 重建 | WI-10, WI-12 |
| Q-002 `LifecycleGate` | 保留物理表，定位为 Lifecycle-owned child table / durable wait state。不要合进 `LifecycleRun` JSONB，因为 open-by-agent、correlation、status、wait polling 和 companion workflow 都要求索引与局部更新 | WI-10 |
| Q-003 `LifecycleSubjectAssociation` | 保留物理表，定位为 indexed relationship table。它承担 subject -> run/agent 反查、anchor -> subject 展示、permission/task/routine/frame context 关联，不适合 JSONB | WI-10 |
| Q-004 current delivery | 从 `LifecycleAgent.current_delivery_*` 移到 `AgentRunDeliveryBinding` 或等价 AgentRun child state。`RuntimeSessionExecutionAnchor` 只保留 immutable evidence；current binding 是不可丢失 state，不叫 projection | WI-06, WI-12 |
| Q-005 RuntimeSession 表命名 | 本轮直接破坏式重命名：`sessions` -> `runtime_sessions`，`session_events` -> `runtime_session_events`，其余 `session_*` trace tables 同步改名。项目未上线，不保留旧 schema 兼容 | WI-02, WI-12 |
| Q-006 AgentFrame surface | 采用单一 canonical typed `surface_json` / equivalent typed document。查询优化可用 generated/read-only projection columns，但不能保留多写源。visible canvas/module refs 变更必须写新 revision | WI-07, WI-12 |
| Q-007 tool approval | 当前不升格为 AgentRun mailbox / command receipt 产品事实。它仍是 RuntimeSession connector approval，但产品路径只能通过 AgentRun-scoped endpoint 解析 current delivery；raw session approval 只作 diagnostic/internal 或删除 | WI-01, WI-04, WI-09 |
| Q-008 冗余物理表 | 已建立初判 ledger。实现时 WI-12 对每个候选表给出删除、合并、降级或保留 migration；不再因为历史表存在而默认保留 | WI-00, WI-12 |

## Repository / Port Inventory

| 对象 | 当前形态 | 分类 | 执行结论 |
| --- | --- | --- | --- |
| `LifecycleRunRepository` | domain repository，管理 `lifecycle_runs` 整行 JSON/text state | independent aggregate repository | 保留，但删除 `context/view_projection` 这类重复 projection 字段；orchestrations/tasks/execution_log 暂作 embedded control-plane state |
| `LifecycleAgentRepository` | domain repository，身份 + current_delivery columns | parent-owned identity repository | 保留身份能力；删除 current delivery 写源，转向 AgentRun delivery binding |
| `AgentFrameRepository` | revision store + `get_current` + append visible mutation | independent revision store + polluted mutation helpers | 保留 revision store；删除 append helper 和 runtime current truth 语义 |
| `LifecycleGateRepository` | gate create/get/open-by-agent/update | Lifecycle child table port | 保留物理表，后续命名应表达 child state，不作为同级 aggregate |
| `LifecycleSubjectAssociationRepository` | create/list_by_subject/list_by_anchor/delete | indexed association table | 保留；其反查职责满足独立表资格 |
| `AgentLineageRepository` | same-run agent control tree | Lifecycle/AgentRun child lineage table | 保留为 agent control tree；不得和 product fork baseline 混用 |
| `AgentRunLineageRepository` | product run fork lineage，含 runtime session ids | AgentRun child lineage/fork record | 保留 product fork record；删除 runtime session id 必填和 runtime indexes |
| `RuntimeSessionExecutionAnchorRepository` | runtime -> run/agent/frame reverse index，当前 upsert/latest | runtime trace reverse index | 保留独立表；改为 insert-once/idempotent create；删除 latest anchor selection API |
| `AgentRunCommandReceiptRepository` | idempotency/outcome repo | independent command receipt table | 保留；receipt 是外部命令幂等事实，不合进 run JSONB |
| `AgentRunMailboxRepository` | queue table repo，带 claim/recover/order/state/payload cleanup | AgentRun-owned child table port | 保留物理表；owner 改为 AgentRun；runtime id 改为 nullable delivery/correlation |
| `SessionPersistence` | mega trait 聚合 meta/events/effects/runtime commands/compaction/projection/lineage | runtime trace store set leaking into services | 删除 application-level mega injection；保留窄 store traits |
| `RepositorySet` / `AgentRunRepositorySet` / `Lifecycle RepositorySet` | service locator style deps | composition root only | 业务层删除大集合依赖，改 use-case deps |

## Physical Table Ledger

| 表/字段 | 分类 | 结论 | 主要原因 |
| --- | --- | --- | --- |
| `lifecycle_runs` | Lifecycle aggregate table | 保留 | control-plane root |
| `lifecycle_runs.context` | duplicate embedded state | 删除 | AgentRun/frame refs、permission/budget 没有足够生产 consumer；重复事实源 |
| `lifecycle_runs.orchestrations` | embedded control-plane state | 保留本轮 JSON/text | 生命周期依附 run，当前主要整体读写；若后续需要节点级 claim/scan 再拆 child table |
| `lifecycle_runs.tasks` | embedded control-plane state | 保留本轮 JSON/text | 有 run 内排序/计划语义，但当前随 run 聚合管理 |
| `lifecycle_runs.execution_log` | append-ish control-plane log | 保留本轮 embedded，后续可评估 event table | 当前不是本轮 P0 blocker |
| `lifecycle_runs.view_projection` | read projection/cache | 删除或移出 aggregate | 可重建 view 不应作为 aggregate 字段 |
| `lifecycle_agents` | run-scoped agent identity | 保留 | AgentRun identity anchor |
| `lifecycle_agents.current_delivery_*` | current binding/state under wrong owner | 删除并迁移 | current delivery 不是 identity 字段，转 `AgentRunDeliveryBinding` |
| `agent_frames` | append-only surface revisions | 保留并重塑 | 能力/认知事实源 |
| `agent_frames.effective_capability_json/vfs_surface_json/mcp_surface_json/...` | multiple write surfaces | 合并为 canonical typed surface | 避免多写源和覆盖式 surface |
| `agent_frame_transitions` | runtime/frame transition evidence | Conditional retain during WI-07 | 与 runtime commands/accepted boundary 合并评估，不能先删 |
| `runtime_session_execution_anchors` | reverse evidence index | 保留，改 immutable | runtime_session_id -> run/agent/frame 查询必须存在 |
| `agent_run_command_receipts` | command idempotency fact | 保留 | 独立幂等查询和 outcome |
| `agent_run_mailbox_messages` | AgentRun queue child table | 保留，修 owner | claim/recover/order/payload cleanup 需要物理表 |
| `agent_run_mailbox_states` | AgentRun queue state | 降级/重命名为 queue state child table | 去掉 runtime_session_id；可保留 pause/backend preference |
| `agent_run_lineages` | product fork record | 保留并精简 | product fork canonical fact；drop runtime ids as required fields |
| `agent_lineages` | agent control tree | 保留为 control tree child table | companion/subagent topology 与 product fork 不同 |
| `lifecycle_gates` | durable wait/gate child table | 保留 | open-by-agent/status/correlation/polling |
| `lifecycle_subject_associations` | indexed relationship table | 保留 | subject reverse lookup and anchor view |
| `sessions` | runtime trace root | 重命名为 `runtime_sessions` | 消除 product session 误读 |
| `session_events` | runtime event log | 重命名为 `runtime_session_events` 并保持 envelope-only | append-only trace fact |
| `session_projection_heads/segments` | runtime context projection | 重命名并保留 | trace projection，可重建但有 performance/state head |
| `session_compactions` | runtime projection compaction audit | 重命名并保留 | projection rebuild/audit |
| `session_runtime_commands` | runtime delivery operation table | 重命名并重语义化 | 不表达用户 command，命名为 runtime delivery command/operation |
| `session_terminal_effects` | runtime terminal effect outbox | 重命名并保留 | terminal side-effect recovery |
| `session_lineage` | internal runtime trace lineage | 重命名并保留 internal-only | product fork 迁出到 AgentRunForkRecord |

## API / Contract / Frontend Inventory

| 层 | 当前事实 | 执行结论 |
| --- | --- | --- |
| raw Session API | `/sessions/{id}` delete、fork、projection rollback、meta patch、tool approval 仍存在 | 产品写入口删除或 internal diagnostic 化；AgentRun scoped route 不复用 raw handler |
| ProjectAgent start contract | `ProjectAgentRunStartResult` 顶层 `runtime_session_id`、`turn_id` | 移入 diagnostic trace meta 或删除；产品导航只用 `run_ref/agent_ref` |
| AgentRun workspace DTO | `delivery_runtime_ref`、`delivery_trace_meta`、stale guard runtime id | runtime ref 只作 diagnostic；stale guard 改 snapshot/run/frame/turn/workspace revision |
| Frontend workspace | `agentRunWorkspaceState.runtime_session_id` 进入 `WorkspaceRuntimeData.sessionId/runtimeSessionId` | 改为 AgentRun target + optional trace meta |
| Tool approval UI | `ToolCallCardShell` 在无 AgentRun target 时 fallback raw session approval | 产品路径删除 fallback；diagnostic view 可显式使用 raw trace approval 或只读 |
| Permission grants | DTO/API 仍展示 `source_runtime_session_id` | 产品 UI 不以 runtime session 为权限事实源，source runtime 只作 audit |

## Execution Readiness

本 inventory 已把规划开放项变成执行结论。实现启动前不再需要额外产品决策；只需要用户确认进入实现阶段。

执行时的硬顺序：

1. WI-01 / WI-09 先清 API/contract/frontend product identity，避免实现期间继续依赖 raw runtime id。
2. WI-02 / WI-12 并行准备 runtime session table/port rename。
3. WI-03 / WI-04 建 admission 和 mailbox owner correction。
4. WI-06 / WI-07 确定 delivery binding 与 AgentFrame surface。
5. WI-05 集成 accepted boundary。
6. WI-08 收束 fork lineage。
7. WI-10 清 Lifecycle redundant state。
8. WI-11 做 RepositorySet cleanup。

若实现中出现新的代码事实推翻本文件结论，必须先回填 `decisions.md`，再修改对应工作项。
