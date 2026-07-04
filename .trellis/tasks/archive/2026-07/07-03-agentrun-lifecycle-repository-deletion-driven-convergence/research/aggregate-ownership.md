# AgentRun / LifecycleRun / LifecycleAgent 聚合归属研究

## 研究边界

- 本研究只读取了当前代码、contracts、migration、tests 相关实现和 `.trellis/spec/` 中的稳定规范。
- 未读取 `.trellis/tasks/` 下任何已有规划文档或 references。
- 结论按第一性原理推导局部最优形态，不以当前任务规划为依据。

## 基本真理

1. 用户需要进入和操作的是一个可导航、可输入、可 fork、可取消、可删除的 Agent 工作台。
   前端规范已经把 AgentRun 作为用户交互界面列入 Project 视图，并明确前端不创建第二套事实源（`.trellis/spec/frontend/architecture.md:5`、`.trellis/spec/frontend/architecture.md:11`）。用户可见执行工作台是 `AgentRunWorkspaceView`，`RuntimeSession` 只作为 trace/diagnostic 视角（`.trellis/spec/frontend/architecture.md:12`、`.trellis/spec/frontend/architecture.md:14`、`.trellis/spec/frontend/architecture.md:166`、`.trellis/spec/frontend/architecture.md:168`）。

2. RuntimeSession 不是产品归属事实源，只是运行 trace / delivery evidence。
   Project overview 明确 `LifecycleRun` 不直接拥有 `RuntimeSession`，runtime trace 通过 `RuntimeSessionExecutionAnchor` 回到 run / agent / frame / orchestration node 坐标（`.trellis/spec/project-overview.md:25`）。代码中 anchor 只记录 launch evidence：`runtime_session_id -> run_id + launch_frame_id + agent_id + optional orchestration node`（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:45`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:68`）。

3. LifecycleRun 是执行生命过程 / control ledger，不是用户工作台本身。
   规范定义 `LifecycleRun` 是 tracked life process / control ledger（`.trellis/spec/backend/workflow/architecture.md:13`、`.trellis/spec/backend/workflow/architecture.md:31`）。代码中的 `LifecycleRun` 持有 `project_id`、`created_by_user_id`、`topology`、`context`、`orchestrations`、`tasks`、`view_projection`、`status`、`execution_log` 和时间戳（`crates/agentdash-domain/src/workflow/entity.rs:159`、`crates/agentdash-domain/src/workflow/entity.rs:160`、`crates/agentdash-domain/src/workflow/entity.rs:168`、`crates/agentdash-domain/src/workflow/entity.rs:170`、`crates/agentdash-domain/src/workflow/entity.rs:172`、`crates/agentdash-domain/src/workflow/entity.rs:173`）。

4. LifecycleAgent 是 run-scoped Agent runtime identity。
   规范定义 `LifecycleAgent` 是 `LifecycleRun` 内的一等 Agent 运行身份，`AgentFrame` 是 effective runtime surface revision（`.trellis/spec/project-overview.md:31`、`.trellis/spec/project-overview.md:33`）。代码中 `LifecycleAgent` 的核心字段是 `id`、`run_id`、`project_id`、`created_by_user_id`、`source`、`project_agent_id`、`status`、`bootstrap_status`、`current_delivery` 和时间戳（`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:166`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:171`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:173`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:175`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:178`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:187`）。

5. AgentRun 的稳定产品身份已经是复合引用，不是单独表主键。
   contract 中 `AgentRunRefDto` 只有 `run_id + agent_id`（`crates/agentdash-contracts/src/runtime/workflow.rs:832`、`crates/agentdash-contracts/src/runtime/workflow.rs:834`、`crates/agentdash-contracts/src/runtime/workflow.rs:835`、`crates/agentdash-contracts/src/runtime/workflow.rs:836`）。application port 也同样使用 `AgentRunRefView { run_id, agent_id }`（`crates/agentdash-application-ports/src/lifecycle_read_model.rs:19`、`crates/agentdash-application-ports/src/lifecycle_read_model.rs:20`、`crates/agentdash-application-ports/src/lifecycle_read_model.rs:21`、`crates/agentdash-application-ports/src/lifecycle_read_model.rs:22`）。

6. 一个 LifecycleRun 可以包含多个 AgentRun 产品节点，但物理表达应是多个 LifecycleAgent，而不是 `LifecycleRun` 内嵌 AgentRun 列表。
   当前 domain 已有 `LifecycleAgentRepository::list_by_run`（`crates/agentdash-domain/src/workflow/repository.rs:75`、`crates/agentdash-domain/src/workflow/repository.rs:78`）。同 run 控制树使用 `AgentLineage`，字段为 `run_id + parent_agent_id + child_agent_id + relation_kind`（`crates/agentdash-domain/src/workflow/agent_lineage.rs:5`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:9`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:11`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:13`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:14`）。

7. 跨 run fork provenance 与同 run agent tree 是两类事实。
   `AgentRunLineage` 表达 forked child AgentRun 到 parent AgentRun/runtime trace boundary 的关系（`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:12`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:14`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:16`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:23`）。迁移中 `agent_run_lineages` 也强制 parent/child run 不同并对 child run/agent 唯一（`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1`、`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:15`、`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:16`、`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:17`）。

8. AgentRun workspace 是 projection，不是新的持久事实源。
   Workspace snapshot 直接聚合 `LifecycleRun`、`LifecycleAgent`、ownership、shell、delivery trace、projection、agent view、frame runtime、subject associations、mailbox、resource surface 和 conversation（`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:20`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:21`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:22`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:23`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:25`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:28`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:36`）。前端状态规范也说 shell/list/action state 来自后端 AgentRun Workspace projection（`.trellis/spec/frontend/state-management.md:144`、`.trellis/spec/frontend/state-management.md:146`、`.trellis/spec/frontend/state-management.md:147`、`.trellis/spec/frontend/state-management.md:159`、`.trellis/spec/frontend/state-management.md:160`）。

9. 当前物理 schema 已经没有独立 `agent_runs` 表。
   初始迁移创建了 `lifecycle_runs`、`lifecycle_agents`、`agent_frames`、`runtime_session_execution_anchors`（`crates/agentdash-infrastructure/migrations/0001_init.sql:40`、`crates/agentdash-infrastructure/migrations/0001_init.sql:254`、`crates/agentdash-infrastructure/migrations/0001_init.sql:282`、`crates/agentdash-infrastructure/migrations/0001_init.sql:533`）。后续 mailbox/receipt/lineage 都通过 `run_id + agent_id` 绑定（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:59`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:61`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:62`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:215`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:222`）。

## 局部最优设计

### 1. 用户可见的一等聚合

用户可见的一等聚合应叫 `AgentRun`，其稳定引用是：

```text
AgentRunRef = { run_id: LifecycleRunId, agent_id: LifecycleAgentId }
```

它不是一张新的 `agent_runs` 表，而是一个产品聚合边界 / command target / workspace read model。它的实体根在持久层由 `LifecycleRun` 和一个 `LifecycleAgent` 共同确定：

- `LifecycleRun` 是生命周期账本、权限/owner、Task plan、orchestration state、execution log 的 owner。
- `LifecycleAgent` 是这个 run 内被操作的 Agent 身份、source、ProjectAgent binding、bootstrap 状态和 current delivery 的 owner。
- `AgentFrame` 是该 agent 的 runtime surface revision。
- `RuntimeSessionExecutionAnchor` 是 RuntimeSession trace 反查到 run/agent/frame/node 的索引。
- `AgentRunWorkspaceView` 是面向用户的 projection。

这与现有 contract 对齐：`AgentRunWorkspaceView` 暴露 `run_ref`、`agent_ref`、`project_id`、`shell`、`delivery_runtime_ref`、`control_plane`、`agent`、`frame_runtime`、`subject_associations`、`resource_surface`、`conversation`、`parent/children`（`crates/agentdash-contracts/src/runtime/workflow.rs:1373`、`crates/agentdash-contracts/src/runtime/workflow.rs:1374`、`crates/agentdash-contracts/src/runtime/workflow.rs:1375`、`crates/agentdash-contracts/src/runtime/workflow.rs:1377`、`crates/agentdash-contracts/src/runtime/workflow.rs:1384`、`crates/agentdash-contracts/src/runtime/workflow.rs:1405`、`crates/agentdash-contracts/src/runtime/workflow.rs:1408`）。

`LifecycleRun` 与 `LifecycleAgent` 不应成为用户一等导航概念。它们是 AgentRun 的内部事实层。外部 URL 和命令应继续使用 `/agent-runs/{run_id}/agents/{agent_id}/...`，当前 route 已经是这个方向（`crates/agentdash-api/src/routes/lifecycle_agents.rs:101`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:109`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:113`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:121`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:145`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:149`）。

### 2. LifecycleRun 是否包含多个 AgentRun

应包含，但包含关系只能通过 `LifecycleAgent` rows 表达：

```text
LifecycleRun(id)
  owns 0..N LifecycleAgent(run_id = id)
  owns 0..N OrchestrationInstance
  owns run-level Task plan facts
  has same-run AgentLineage edges between LifecycleAgent rows
```

每个 `LifecycleAgent` 对外投影为一个 `AgentRunRef { run_id, agent_id }`。当前 `LifecycleRunView.agents: Vec<AgentRunView>` 已经这么做（`crates/agentdash-contracts/src/runtime/workflow.rs:1506`、`crates/agentdash-contracts/src/runtime/workflow.rs:1516`），read model builder 也是 `list_by_run` 后映射 agent views（`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:60`、`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:65`、`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:375`、`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:377`）。

双向引用最小化规则：

- `LifecycleAgent.run_id -> LifecycleRun.id` 是唯一强归属边。
- `AgentFrame.agent_id -> LifecycleAgent.id` 是 surface revision 边。
- `RuntimeSessionExecutionAnchor.runtime_session_id -> run_id + agent_id + launch_frame_id + optional node` 是 runtime trace 索引。
- `AgentLineage(run_id, parent_agent_id, child_agent_id)` 表达同 run 控制树。
- `AgentRunLineage(parent_run_id, parent_agent_id, child_run_id, child_agent_id)` 只表达跨 run fork provenance。
- `LifecycleRun` 不应再内嵌 `agent_runs`、`frame_refs`、`main_agent_run_id` 这类反向索引。

当前列表 API 已经按这个模型工作：先按 project 列出 runs，再 `list_by_run` 取 agents 和 `agent_lineage_repo.list_by_run` 建 forest，只把未作为 child 的 agents 作为主 AgentRun entry（`crates/agentdash-api/src/routes/lifecycle_agents.rs:220`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:224`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:234`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:246`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:269`）。

### 3. 必须落在哪些事实源上

#### AgentRun scoped facts

这里的 AgentRun scoped 指以 `AgentRunRef { run_id, agent_id }` 为 scope，不代表单独 `AgentRun` 表。

- command target identity：`run_id + agent_id`，与 contracts / routes 保持一致（`crates/agentdash-contracts/src/runtime/workflow.rs:834`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:101`）。
- command idempotency：receipt 应 scoped 到 AgentRunRef。当前 `AgentRunCommandReceipt` 使用 `scope_kind + scope_key + client_command_id` 做 claim（`crates/agentdash-domain/src/workflow/command_receipt.rs:98`、`crates/agentdash-domain/src/workflow/command_receipt.rs:100`、`crates/agentdash-domain/src/workflow/command_receipt.rs:103`、`crates/agentdash-domain/src/workflow/command_receipt.rs:144`、`crates/agentdash-domain/src/workflow/command_receipt.rs:145`），迁移也有唯一约束（`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:27`、`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:28`、`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:31`）。局部最优是约定 `scope_kind = "agent_run"`，`scope_key = canonical(run_id, agent_id)`。
- mailbox messages / state：必须 keyed by `run_id + agent_id`，因为它是 AgentRun 输入与调度队列（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:353`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:355`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:356`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:390`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:392`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:413`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:415`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:416`）。
- cross-run fork provenance：`AgentRunLineage` 必须保留为 AgentRun scoped fact，因为它连接两个 AgentRunRef 并引用 fork boundary runtime sessions（`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:14`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:16`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:23`、`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:24`）。
- command outcomes that return accepted refs：`AgentRunAcceptedRefs` 包含 run/agent/frame/runtime/turn refs，是命令结果 fact，不应反推写入生命周期主状态（`crates/agentdash-domain/src/workflow/command_receipt.rs:86`、`crates/agentdash-domain/src/workflow/command_receipt.rs:88`、`crates/agentdash-domain/src/workflow/command_receipt.rs:90`、`crates/agentdash-domain/src/workflow/command_receipt.rs:92`、`crates/agentdash-domain/src/workflow/command_receipt.rs:93`）。

#### Lifecycle facts

- `LifecycleRun`: `id`、`project_id`、`created_by_user_id`、`topology`、`status`、`orchestrations`、`tasks`、`execution_log`、timestamps。Task plan fact 已在 `LifecycleRun.tasks` 上（`crates/agentdash-domain/src/workflow/entity.rs:168`、`crates/agentdash-domain/src/workflow/entity.rs:170`、`crates/agentdash-domain/src/workflow/entity.rs:419`、`crates/agentdash-domain/src/workflow/entity.rs:442`）。
- `LifecycleAgent`: `run_id`、`project_id`、agent owner/origin、`source`、`project_agent_id`、`status`、`bootstrap_status`、`current_delivery`（`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:171`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:173`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:175`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:178`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:180`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:181`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:184`、`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:187`）。
- `AgentFrame`: effective capability、context slice、VFS surface、MCP surface、execution profile、visible runtime grants（`crates/agentdash-domain/src/workflow/agent_frame.rs:6`、`crates/agentdash-domain/src/workflow/agent_frame.rs:10`、`crates/agentdash-domain/src/workflow/agent_frame.rs:15`、`crates/agentdash-domain/src/workflow/agent_frame.rs:17`、`crates/agentdash-domain/src/workflow/agent_frame.rs:19`、`crates/agentdash-domain/src/workflow/agent_frame.rs:21`、`crates/agentdash-domain/src/workflow/agent_frame.rs:23`、`crates/agentdash-domain/src/workflow/agent_frame.rs:26`、`crates/agentdash-domain/src/workflow/agent_frame.rs:33`）。
- `RuntimeSessionExecutionAnchor`: runtime trace launch evidence，不能成为 ownership 或 product URL 事实源（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:25`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:30`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:31`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:32`、`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:33`）。
- `AgentLineage`: same-run parent/child control tree（`crates/agentdash-domain/src/workflow/agent_lineage.rs:5`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:11`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:13`、`crates/agentdash-domain/src/workflow/agent_lineage.rs:14`）。
- `LifecycleSubjectAssociation`: subject 到 whole run 或 agent 的关联，不应塞入 AgentRun workspace projection cache（`crates/agentdash-domain/src/workflow/repository.rs:103`、`crates/agentdash-domain/src/workflow/repository.rs:105`、`crates/agentdash-domain/src/workflow/repository.rs:109`）。
- `LifecycleGate`: wait/open gate fact，workspace 只投影 waiting items（`crates/agentdash-domain/src/workflow/repository.rs:118`、`crates/agentdash-domain/src/workflow/repository.rs:121`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:188`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:191`）。

#### Projection only

这些字段应由 read model / query service 组装，不应回写为 domain truth：

- `AgentRunWorkspaceShell.display_title/title_source/workspace_status/delivery_status/last_turn_id/last_activity_at`（`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:65`、`crates/agentdash-application-agentrun/src/agent_run/workspace/types.rs:66`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:531`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:548`）。
- workspace state code / active turn / delivery status projection（`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:7`、`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:10`、`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:29`、`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:70`）。
- `AgentRunWorkspaceView`、`AgentRunWorkspaceListEntry`、`AgentRunListChild`、subagent counts、subject labels、delivery trace meta、resource surface summary（`crates/agentdash-contracts/src/runtime/workflow.rs:1373`、`crates/agentdash-contracts/src/runtime/workflow.rs:1675`、`crates/agentdash-contracts/src/runtime/workflow.rs:1701`、`crates/agentdash-contracts/src/runtime/workflow.rs:1726`、`crates/agentdash-contracts/src/runtime/workflow.rs:1729`、`crates/agentdash-contracts/src/runtime/workflow.rs:1735`）。
- `LifecycleRunView.runtime_trace_refs` 和 `AgentRunView.delivery_runtime_ref` 是 anchor/current_delivery projection（`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:190`、`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:194`、`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:375`、`crates/agentdash-application-lifecycle/src/lifecycle/run_view_builder.rs:386`）。
- conversation snapshot、command availability、resource diagnostics。当前 workspace query 已在一次 resolve 中计算它们（`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:172`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:188`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:230`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:232`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:263`）。

### 4. 最小仓储 / 表 / port 形态

#### 最小表

不新增 `agent_runs` 表。保留或收束到以下物理事实表：

- `lifecycle_runs`: run ledger。必要列为 `id`、`project_id`、`created_by_user_id`、`topology`、`orchestrations`、`tasks`、`status`、`execution_log`、timestamps。`context` 是否保留取决于是否有真实 run-level permission/budget facts；当前 agent refs 子字段不应保留。`view_projection` 不应保留在 aggregate 表。
- `lifecycle_agents`: run-scoped agent identity。必要列为 `id`、`run_id`、`project_id`、`created_by_user_id`、`source`、`project_agent_id`、`status`、`bootstrap_status`、`current_delivery_*`、timestamps。current delivery 已由迁移添加并在 migration 中约束状态枚举（`crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:1`、`crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:14`、`crates/agentdash-infrastructure/migrations/0017_lifecycle_agent_current_delivery_binding.sql:27`）。
- `agent_frames`: frame revision surface。
- `runtime_session_execution_anchors`: runtime trace launch evidence。
- `agent_lineages`: same-run control tree。
- `agent_run_lineages`: cross-run fork provenance。
- `lifecycle_subject_associations`: subject/run/agent relation。
- `lifecycle_gates`: wait/gate facts。
- `agent_run_mailbox_messages` / `agent_run_mailbox_states`: AgentRun scoped command queue/state.
- `agent_run_command_receipts`: AgentRun scoped command idempotency/outcome receipt.

#### 最小 repository traits

保留 domain-level repository，删除面向 AgentRun 的重复 repo 概念：

- `LifecycleRunRepository`: `create/get_by_id/list_by_ids/list_by_project/update/delete`（`crates/agentdash-domain/src/workflow/repository.rs:65`、`crates/agentdash-domain/src/workflow/repository.rs:66`、`crates/agentdash-domain/src/workflow/repository.rs:69`、`crates/agentdash-domain/src/workflow/repository.rs:71`）。
- `LifecycleAgentRepository`: `create/get/list_by_run/update`（`crates/agentdash-domain/src/workflow/repository.rs:75`、`crates/agentdash-domain/src/workflow/repository.rs:76`、`crates/agentdash-domain/src/workflow/repository.rs:78`、`crates/agentdash-domain/src/workflow/repository.rs:79`）。
- `AgentFrameRepository`: core frame reads/writes. Surface-specific append methods should be considered separate surface ports because they are not AgentRun aggregate identity（`crates/agentdash-domain/src/workflow/repository.rs:83`、`crates/agentdash-domain/src/workflow/repository.rs:86`、`crates/agentdash-domain/src/workflow/repository.rs:88`、`crates/agentdash-domain/src/workflow/repository.rs:93`）。
- `RuntimeSessionExecutionAnchorRepository`: anchor upsert/delete/find/list，作为 runtime trace 索引（`crates/agentdash-domain/src/workflow/repository.rs:153`、`crates/agentdash-domain/src/workflow/repository.rs:155`、`crates/agentdash-domain/src/workflow/repository.rs:159`、`crates/agentdash-domain/src/workflow/repository.rs:164`、`crates/agentdash-domain/src/workflow/repository.rs:169`、`crates/agentdash-domain/src/workflow/repository.rs:181`）。
- `AgentLineageRepository` and `AgentRunLineageRepository`: same-run tree 与 cross-run fork provenance 分开（`crates/agentdash-domain/src/workflow/repository.rs:126`、`crates/agentdash-domain/src/workflow/repository.rs:132`、`crates/agentdash-domain/src/workflow/repository.rs:136`、`crates/agentdash-domain/src/workflow/repository.rs:148`）。
- `AgentRunMailboxRepository` and `AgentRunCommandReceiptRepository`: AgentRun command plane（`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:438`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:451`、`crates/agentdash-domain/src/agent_run_mailbox/mod.rs:504`、`crates/agentdash-domain/src/workflow/command_receipt.rs:144`）。

#### 最小 application ports

用窄 port 替代大而全 repository set：

1. `AgentRunResolver`:
   - input: `AgentRunRef`, optional `project_id`, optional viewer.
   - output: `{ run: LifecycleRun, agent: LifecycleAgent, ownership }`.
   - responsibilities: project membership check、agent belongs to run、owner model。

2. `AgentRunDeliveryResolver`:
   - input: `AgentRunRef`.
   - output: `current_delivery + current_frame + runtime session state`.
   - responsibilities: 只从 `LifecycleAgent.current_delivery`、anchors、frames 和 session boundary 解析，不读取 projection cache。

3. `AgentRunWorkspaceReadModelPort`:
   - input: `AgentRunRef`, viewer.
   - output: `AgentRunWorkspaceView` / list projection.
   - owns shell/control-plane/conversation/resource surface projection.

4. `AgentRunCommandPort`:
   - submit/fork/fork-submit/cancel/delete/mailbox operations。
   - internally uses receipt, mailbox, lifecycle, session, fork materialization ports.
   - product command target remains `AgentRunRef`.

5. `LifecycleReadModelQueryPort`:
   - already exists and should remain lifecycle-owned（`crates/agentdash-application-ports/src/lifecycle_read_model.rs:133`、`crates/agentdash-application-ports/src/lifecycle_read_model.rs:150`）。

The current `AgentRunRepositorySet` pulls many unrelated repositories into every AgentRun service（`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:40`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:41`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:68`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:86`）。It should become composition-only at API bootstrap, not a service dependency type.

## 删除清单

1. 删除或降级 `LifecycleRun.context` 中的 AgentRun 反向索引。
   `LifecycleContext` 当前包含 `main_agent_run_id`、`agent_runs`、`frame_refs`（`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:15`、`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:18`、`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:20`、`crates/agentdash-domain/src/workflow/value_objects/orchestration.rs:22`），但真实查询从 `lifecycle_agents`、`agent_frames`、anchors、lineage 得到。保留这些字段会让 `LifecycleRun` 与 `LifecycleAgent/AgentFrame` 双向引用。局部最优是删除 `main_agent_run_id`、`agent_runs`、`frame_refs`；若 `permission_scope/budget` 没有生产 consumer，则进一步删除整个 `context` 列和类型。

2. 删除或移出 `LifecycleRun.view_projection`。
   `view_projection` 在 domain 和 migration 中存在（`crates/agentdash-domain/src/workflow/entity.rs:172`、`crates/agentdash-infrastructure/migrations/0003_lifecycle_orchestration_contract.sql:4`），但代码搜索只发现仓储 roundtrip、fork/materialization copy 和测试使用，没有业务读路径。projection 应由 read model 构建或放独立 cache，不应作为 aggregate 字段。

3. 删除 fork/materialization 中复制 parent `context/view_projection` 到 child run 的逻辑。
   现有 `agent_run_lineage_repository.rs` 和测试支撑代码复制 `parent_run.view_projection` / context。fork 的 child run 应拥有新的 lifecycle ledger，RuntimeSession projection 继承已经由 session branching 负责；LifecycleRun 不应复制旧 projection/cache。

4. 降级 `AgentRunRepositorySet` 和 `LifecycleRepositorySet` 为 composition wiring。
   当前 `AgentRunRepositorySet` 聚合从 Project、Canvas、Workspace、Backend、Settings 到 Lifecycle、Mailbox 等大量仓储（`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:41`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:68`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:77`、`crates/agentdash-application-agentrun/src/agent_run_repository_set.rs:86`）。具体服务应接收窄 deps，例如 delete 事实上只需要 run/agent/anchor repo 和 session core（`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:26`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:28`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:30`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:43`）。

5. 重命名或拆分 `routes/lifecycle_agents.rs`。
   文件名是 lifecycle_agents，但内容是 AgentRun 产品路由：list、workspace、composer-submit、fork、mailbox、cancel、runtime control 等（`crates/agentdash-api/src/routes/lifecycle_agents.rs:90`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:93`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:101`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:109`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:145`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:165`）。应拆成 `routes/agent_runs.rs`，只把真正 LifecycleRun/LifecycleAgent diagnostics 留在 lifecycle routes。

6. 降级 `agent_run/lifecycle_read_model_facade.rs`。
   该文件只是 re-export lifecycle read model port，并承认 lifecycle projection 由 `agentdash-application-lifecycle` 拥有（`crates/agentdash-application-agentrun/src/agent_run/lifecycle_read_model_facade.rs:1`、`crates/agentdash-application-agentrun/src/agent_run/lifecycle_read_model_facade.rs:3`、`crates/agentdash-application-agentrun/src/agent_run/lifecycle_read_model_facade.rs:7`）。局部最优是直接依赖 `agentdash-application-ports::lifecycle_read_model`，避免 AgentRun crate 再制造一层“facade owner”错觉。

7. 清理 session-first presentation read model 的产品入口。
   `AgentRunPresentationReadModelQuery::runtime_session_trace/session_runtime_control` 从 runtime session id 出发再反查 lifecycle（`crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:92`、`crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:124`、`crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:138`、`crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:162`）。这类路径只应是 diagnostics，不应服务 AgentRun 产品工作台。

8. 区分 same-run `AgentLineage` DTO 和 cross-run `AgentRunLineage` DTO。
   当前 API 用 `AgentRunLineageRef` 同时承载 same-run parent/children UI（`crates/agentdash-contracts/src/runtime/workflow.rs:1652`、`crates/agentdash-contracts/src/runtime/workflow.rs:1657`），但 same-run 数据来自 `AgentLineage`（`crates/agentdash-api/src/routes/lifecycle_agents.rs:548`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:556`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:564`）。建议将 same-run DTO 命名为 `AgentControlTreeRef` 或 `AgentRunTreeRef`，保留 `AgentRunLineage` 给 fork provenance。

9. 删除旧的 `LifecycleAgent.current_frame_id` 残余假设。
   migration 已删除该列（`crates/agentdash-infrastructure/migrations/0020_drop_lifecycle_agent_current_frame.sql:1`、`crates/agentdash-infrastructure/migrations/0020_drop_lifecycle_agent_current_frame.sql:2`）。所有 current frame 解析都应通过 current delivery selection 或 `AgentFrameRepository::get_current(agent.id)`（`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:402`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:407`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:415`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:417`）。

## 迁移 / 实施顺序

1. 固化术语和 invariant。
   - `AgentRunRef = run_id + agent_id`。
   - 用户产品层只说 AgentRun / AgentRun Workspace。
   - lifecycle 内部层说 LifecycleRun / LifecycleAgent / AgentFrame / Anchor。
   - RuntimeSession 只说 trace/delivery evidence。

2. 先做只读收束，不动行为。
   - 新增 `AgentRunResolver`，集中校验 run exists、project match、agent belongs to run、ownership。
   - 将 workspace query、composer/fork/cancel/delete/mailbox routes 先改为使用 resolver。
   - 保持现有 response contract 不变。

3. 拆窄 service deps。
   - delete service 已经展示了窄 deps 的形态（`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:26`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:43`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:56`）。
   - fork service 当前 deps 仍较宽（`crates/agentdash-application-agentrun/src/agent_run/fork.rs:36`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:44`），可先拆 `parent resolver`、`receipt port`、`fork materialization port`。
   - project start service 当前通过 `AgentLaunchIntent` 创建 lifecycle materialization，这是正确主路径，应保留（`crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:365`、`crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:376`、`crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:389`、`crates/agentdash-application-agentrun/src/agent_run/project_agent_start.rs:411`）。

4. 删除 duplicate aggregate fields。
   - 从 domain `LifecycleContext` 删除 `main_agent_run_id`、`agent_runs`、`frame_refs`。
   - 修改 repository JSON parse/serialize 和 tests。
   - 如果 `permission_scope/budget` 没有 production consumer，则删除整个 `LifecycleContext` / `lifecycle_runs.context`。
   - 添加 migration drop/prune；项目未上线，可以选择 forward migration 或重写初始 migration，但要通过 migration history 检查。

5. 删除 `view_projection` aggregate 字段。
   - 移除 domain 字段、repository columns、roundtrip tests、fork/materialization copy。
   - migration drop `lifecycle_runs.view_projection`。
   - 如需要缓存，将其作为独立 read-model cache 表，不被 command/reducer 读取。

6. 拆 route 文件和 DTO 命名。
   - `routes/lifecycle_agents.rs` 拆为 `routes/agent_runs.rs`。
   - same-run lineage DTO 改名，不再与 cross-run `AgentRunLineage` 共名。
   - raw session trace/control 入口移动到 diagnostics/session trace routes。

7. 收紧 DB 约束。
   - `lifecycle_agents.run_id -> lifecycle_runs(id) ON DELETE CASCADE`。
   - `agent_frames.agent_id -> lifecycle_agents(id)`。
   - mailbox messages/states 已有 run/agent FK 和 cascade，应保留（`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:170`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:174`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:179`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:183`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:229`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:233`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:238`、`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:242`）。
   - command receipt 的 `scope_kind/scope_key` 增加 typed helper，所有 AgentRun command 统一使用 canonical AgentRunRef scope。

8. 回归验证。
   - domain tests: LifecycleRun 不再保存 AgentRun refs/view projection。
   - repository tests: lifecycle_runs roundtrip 不含删除字段。
   - application tests: project start、fork、fork-submit、delete、mailbox、workspace query。
   - API tests: AgentRun routes、list root/children、same-run tree、cross-run fork redirect。
   - frontend tests: AgentRun workspace/list 仍只依赖 generated contracts 和 AgentRunRef。

## 需要验证的代码事实

1. `LifecycleRun.view_projection` 是否存在任何非测试、非 roundtrip、非 copy 的业务读取。
   当前 `rg view_projection` 显示主要是 spec、domain 字段、migration、repository serialize/parse、roundtrip tests、fork/materialization copy。实施前应再跑一次精确搜索。

2. `LifecycleContext.permission_scope/budget` 是否有生产 consumer。
   当前 `LifecycleContext` 的 `main_agent_run_id/agent_runs/frame_refs` 是重复索引；`permission_scope/budget` 是否值得保留需要用 `rg "permission_scope|budget"` 结合调用链确认。若无消费，删除整个 context 是更简形态。

3. command receipt scope 是否所有 AgentRun command 都 canonical。
   domain 是 `scope_kind/scope_key/client_command_id`（`crates/agentdash-domain/src/workflow/command_receipt.rs:98`、`crates/agentdash-domain/src/workflow/command_receipt.rs:100`、`crates/agentdash-domain/src/workflow/command_receipt.rs:103`），需要确认 `project_agent_start`、composer submit、fork、fork-submit、mailbox promote/delete/resume、cancel 都使用同一 `AgentRunRef` scope string。

4. `LifecycleAgent.current_delivery` 与 anchors 的一致性。
   current delivery 是当前投递指针；anchors 是 runtime trace history。workspace query 只有在 `agent.current_delivery` 存在时才调用 delivery selection（`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:350`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:355`、`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:358`）。需要验证所有 runtime launch / terminal / lost 路径都会更新 current delivery status。

5. 删除 AgentRun 时 cascade 是否覆盖所有 run-owned rows。
   delete service 先收集 anchors 和 agents current delivery，再删 runtime sessions，最后删 lifecycle run（`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:83`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:84`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:99`、`crates/agentdash-application-agentrun/src/agent_run/delete_command.rs:108`）。需要验证 `lifecycle_agents`、`agent_frames`、anchors、mailbox、receipts、lineages、gates、subject associations 都有正确 FK/cascade 或显式 cleanup。

6. same-run AgentLineage 与 cross-run AgentRunLineage 的 API 表达是否混淆。
   list/detail 当前从 `AgentLineage` 构建 parent/children（`crates/agentdash-api/src/routes/lifecycle_agents.rs:548`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:556`、`crates/agentdash-api/src/routes/lifecycle_agents.rs:602`）。fork service 则通过 `AgentRunForkMaterializationPort` 写 cross-run lineage（`crates/agentdash-application-agentrun/src/agent_run/fork.rs:291`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:294`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:296`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:299`）。需要拆清 DTO 名称和 store 命名。

7. AgentFrame current resolution 是否完全脱离 `LifecycleAgent.current_frame_id`。
   migration 已 drop current_frame_id（`crates/agentdash-infrastructure/migrations/0020_drop_lifecycle_agent_current_frame.sql:1`、`crates/agentdash-infrastructure/migrations/0020_drop_lifecycle_agent_current_frame.sql:2`），但需要确认所有 launch/envelope/context path 都不再依赖旧字段。

8. `AgentRunRepositorySet` 的拆窄不会破坏 bootstrap composition。
   当前 `AgentRunRepositorySet` 是 API bootstrap 方便注入的大容器；拆窄时要保留 bootstrap 层 composition，但禁止 application service 直接要求整个 set。

## 结论

局部最优不是新增 `AgentRun` 持久 aggregate，也不是把 `LifecycleAgent` 暴露成用户主概念。最简正确形态是：

```text
用户产品聚合: AgentRun = AgentRunRef(run_id, agent_id) + Workspace/Command projection
持久控制根: LifecycleRun
run 内 agent 身份: LifecycleAgent
runtime surface: AgentFrame
runtime trace evidence: RuntimeSessionExecutionAnchor
same-run tree: AgentLineage
cross-run fork: AgentRunLineage
command plane: AgentRun mailbox + command receipts
```

`LifecycleRun` 可以包含多个 AgentRun，但只通过 `LifecycleAgent` rows 和 `AgentLineage` 表达。`LifecycleRun` 不应再内嵌 AgentRun refs、frame refs 或 view projection。所有用户可见状态都应由 AgentRun workspace read model 从 lifecycle facts、mailbox facts、session trace facts和 frame surface facts 投影出来。
