# AgentFrame / Context Surface 研究结论

## 研究边界

本研究只从代码、migration、contracts 与 `.trellis/spec/` 推导；未读取 `.trellis/tasks/` 下任何既有规划文档或 references。目标不是复述现状，而是给出 AgentFrame / capability surface / context delivery / ContextFrame 的局部最优形态。

## 基本真理

1. Agent 执行只需要一个闭包后的 runtime surface。
   这个 surface 必须回答：当前 agent 是谁、当前 frame revision 是哪一版、模型能看到哪些能力、VFS、MCP、执行器配置和上下文摘要。connector 不应理解 ProjectAgent、Routine、Companion、Workflow 等 application 来源差异；spec 已定义 `ExecutionContext` 只是 connector-facing projection，事实来自 `AgentFrame`、`FrameLaunchEnvelope` 和 `LaunchPlan`（`.trellis/spec/backend/session/execution-context-frames.md:3`, `.trellis/spec/backend/session/execution-context-frames.md:18`）。

2. launch evidence 和 current surface 是两类事实。
   `RuntimeSessionExecutionAnchor.launch_frame_id` 记录 runtime session 创建时刻的 frame，不被后续 revision 覆盖；查询最新 surface 仍应按 `agent_id` 取 current frame（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20`, `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:25`）。

3. current state 不是独立事实源。
   AgentFrame revision 是 append-only 序列，current 只是 `agent_id` 下最大 revision 的查询投影。数据库已有 `(agent_id, revision)` 唯一索引（`crates/agentdash-infrastructure/migrations/0001_init.sql:1034`），`LifecycleAgent.current_frame_id` 已被删除（`crates/agentdash-infrastructure/migrations/0020_drop_lifecycle_agent_current_frame.sql:1`）。

4. capability / VFS / MCP / executor / context 是同一个 surface closure 的不同 facet。
   当前代码已经要求 launch surface 中 `capability_state.vfs.active == vfs`、`capability_state.tool.mcp_servers == mcp_servers`（`crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs:80`）。因此它们不能成为多个可写事实源。

5. ContextFrame 是投递投影，不是上游事实源。
   ContextFrame 由 launch surface、context discovery、pending runtime transition、hook queue 在 turn preparation 阶段组装，connector 消费 `context_frames` 和 `context_delivery_plan`（`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:335`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:380`）。

6. Mailbox / runtime command 是调度事实，不是 surface 事实。
   mailbox 负责 durable message intake 与 boundary scheduling（`.trellis/spec/backend/session/agentrun-mailbox.md:3`）。`RuntimeCommandRecord` 记录 pending transition 的投递状态（`crates/agentdash-spi/src/session_persistence.rs:390`），不应同时充当 current capability surface。

7. fork 是在某个父 current frame 上拍快照。
   fork 入口先通过 current delivery selection 得到 parent current frame（`crates/agentdash-application-agentrun/src/agent_run/fork.rs:538`），materialization 再把 parent frame surface 复制到 child revision 1（`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:130`）。fork baseline 必须是这个 parent current frame，而不是 parent runtime session 的 launch frame。

## 推荐设计

### 1. AgentFrame 的不可约事实

AgentFrame 应只承载一个 append-only revision 的 runtime surface closure 和 provenance：

```rust
AgentFrame {
    id: Uuid,
    agent_id: Uuid,
    revision: i32,
    source_frame_id: Option<Uuid>,
    surface_json: AgentFrameSurface,
    created_by_kind: String,
    created_by_id: Option<String>,
    created_at: DateTime<Utc>,
}

AgentFrameSurface {
    capability_state: CapabilityState,
    execution_profile: AgentConfig,
    context_summary: Option<FrameContextBundleSummary>,
}
```

`CapabilityState` 是 tool / VFS / MCP / skill / memory / companion / workspace module 等 capability dimensions 的闭包后状态。VFS 和 MCP 可以作为 typed accessor 暴露，但不应作为独立可写 JSON 列覆盖 `CapabilityState`。

AgentFrame 内不应放：

- runtime session id、turn id、active turn、delivery status。这些属于 `RuntimeSessionExecutionAnchor`、`LifecycleAgent.current_delivery` 或 runtime trace。
- current pointer。current 是 `get_current(agent_id)` 查询投影。
- mailbox message、command receipt、runtime command 状态。这些属于调度/恢复边界。
- permission grant rows。grant 是授权/准入事实；若 grant 改变 runtime surface，应写新 frame revision。
- ContextFrame / ContextDeliveryPlan。它们是 per-turn 投影。
- hook runtime live state。hook runtime 以 `run_id + agent_id + frame_id` 为主语读取 frame surface，但不是 frame 内容（`crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:5`）。
- 运行期追加的 `visible_*` 列。Canvas / workspace module exposure 应是 capability dimension effect 后的新 revision，不应 in-place 改旧 revision。

### 2. Frame 模型：append-only revision + current projection

最小正确模型是“两者组合”，但只有 append-only revision 是持久事实：

- `agent_frames` 保存 revision 日志。
- `get_current(agent_id)` 按 `revision DESC, created_at DESC` 查询 current（`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:475`）。
- `source_frame_id` 表达本 revision 从哪一版 surface 派生。普通同 agent 更新可指向上一版 current；fork child revision 1 可指向 parent current frame。
- current state 是 read model，不落 `lifecycle_agents.current_frame_id`，也不做 mutable row update。

现有 `AgentFrameBuilder` 已接近这个方向：`build_uncommitted` 从 current revision + 1 写新 frame（`crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:240`）。需要收敛的是它当前仍 carry-forward 多个独立列（`crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:248`）并复制 mutable visible refs（`crates/agentdash-application-agentrun/src/agent_run/frame/builder.rs:273`）。

### 3. capability / VFS / MCP / executor / context 的建模

推荐单一 canonical surface：

- `AgentFrameSurface.capability_state` 是 capability 事实源。
- VFS 是 `capability_state.vfs.active`。
- MCP executable server set 是 `capability_state.tool.mcp_servers`。
- executor 是 `surface.execution_profile`，属于执行 profile，不属于 capability dimension。
- context 在 frame 中只存 `context_summary` / digest / provenance；完整 ContextFrame 由 launch context discovery 和 turn preparation 生成。

如果为了查询性能保留物理列，也必须是 generated projection 或只读缓存，不能由不同入口独立写入。当前 `project_capability_state_from_frame` 先读 `effective_capability_json`，再用 `vfs_surface_json` / `mcp_surface_json` 覆盖（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:42`）；这正是需要删除的双源形态。`capability_state_to_frame_surfaces` 又把同一 state 拆回三列（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:76`），说明单一 typed surface 已经具备替代条件。

dimension 规则应保持：

- declaration/effect 使用 `RuntimeCapabilityTransition { declarations, effects }`，不保存完整 surface（`.trellis/spec/backend/capability/capability-dimension-pipeline.md:30`）。
- base 是声明式真值，modifier 是运行时增量（`.trellis/spec/backend/capability/capability-dimension-pipeline.md:81`）。
- VFS / MCP / workspace module / skill / memory 的 ownership 按 dimension matrix 走（`.trellis/spec/backend/capability/capability-dimension-pipeline.md:98`）。

### 4. ContextFrame emission 的唯一触发事实

模型可见 ContextFrame emission 的唯一触发应是：

> 一个 turn 被 `FrameLaunchEnvelope -> LaunchPlan -> TurnPreparer` 成功准备，并在 connector accepted 后提交。

现有路径已经支持这个结论：

- `TurnPreparer` 先闭包 launch plan、assembled tools、capability state，再决定 startup context、memory、guidelines、pending transition、hook queued frames（`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:119`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:155`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:254`）。
- `TurnPreparer` 对 frames 去空、去重、加 target，并构造 `ContextDeliveryPlan`（`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:551`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:589`）。
- `TurnCommitter` 在 connector accepted 后把 pending transition frames 和 accepted context frames 写为 session events（`crates/agentdash-application-runtime-session/src/session/launch/commit.rs:56`）。
- 事件形式是 `SessionMetaUpdate { key: "context_frame" }`，用于 trace / UI / audit（`crates/agentdash-application-runtime-session/src/session/eventing.rs:367`）。

因此不需要 `ContextDeliveryRecord` 表。`ContextDeliveryPlan` 是 per-turn deterministic projection：从 deduped `ContextFrame[] + connector profile + turn_id` 可重算。只有在未来需要“跨进程 exactly-once 外部投递确认”时，才引入 delivery receipt；那也应记录 external ack，不记录第二份 context 内容。

hook runtime 可以 enqueue turn-start notice（`crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:492`），但模型可见消费仍应由下一次 `TurnPreparer` drain（`crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:511`）。hook 直接 `emit_context_frame` 的路径应视为 audit/wakeup trace；若要影响模型，必须进入 mailbox/turn preparation。

### 5. fork baseline 与 frame baseline

frame baseline 是“本 revision surface 的来源 frame”。fork baseline 是这个概念的跨 run 特例。

推荐关系：

- parent fork baseline = parent agent 的 current frame。
- child initial frame = revision 1，`source_frame_id = parent_current_frame_id`。
- child runtime anchor 的 `launch_frame_id = child_initial_frame_id`。
- `AgentRunLineage` 继续记录 parent/child run/agent/runtime session 与 fork point；若 read model 需要避免查询 child revision 1，可增加 `child_frame_id`，但不要引入独立 fork baseline 表。

这让 fork 的 surface 继承和 revision provenance 共用一条语义，不再让 `RuntimeSessionExecutionAnchor.launch_frame_id` 承担 baseline 角色。当前 fork 已经用 parent current frame 作为输入（`crates/agentdash-application-agentrun/src/agent_run/fork.rs:548`），但 lineage 表只记录 runtime session，没有 frame baseline（`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1`）。

### 6. 最小仓储 / 表 / port 形态

最小表：

- `lifecycle_runs`：run identity / project / owner。
- `lifecycle_agents`：agent identity、source、bootstrap_status、current_delivery binding。
- `agent_frames`：append-only revisions，含 `source_frame_id` 与 `surface_json`。
- `runtime_session_execution_anchors`：runtime session -> run / agent / launch frame / orchestration node。
- `agent_run_lineages`：fork provenance，必要时加 `child_frame_id`，不加独立 baseline 表。
- `agent_run_mailbox_messages` / command receipts：message scheduling 与 recovery。
- `permission_grants`：授权和 admission facts；grant effect 通过新 frame revision 体现。

最小 repository：

- `LifecycleRunRepository`
- `LifecycleAgentRepository`
- `AgentFrameRepository { create, get, get_current, list_by_agent }`
- `RuntimeSessionExecutionAnchorRepository { upsert, find_by_session, list_by_run, list_by_agent }`
- `AgentRunLineageRepository`
- `AgentRunMailboxRepository`
- `PermissionGrantRepository`

`AgentFrameRepository` 不应有 `append_visible_canvas_mount` / `append_visible_workspace_module_ref` 这类 in-place update 方法（当前接口在 `crates/agentdash-domain/src/workflow/repository.rs:83`）。

最小 port：

- `FrameSurfaceMaterializationPort`：从 command / project agent / workflow node / existing surface 生成完整 `AgentFrameSurface` 并写 revision。
- `FrameLaunchEnvelopePort`：把 current frame + command intent + runtime context discovery 变成 launch-ready envelope（`crates/agentdash-application-ports/src/frame_launch_envelope.rs:111`）。
- `CurrentRuntimeSurfaceQueryPort`：通过 runtime session anchor 找 agent，再取 current frame，返回 typed surface（`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:310`）。
- `RuntimeSurfaceAdoptionPort`：把新 frame surface 注入 live runtime tools，不写 frame。
- `RuntimeSessionTurnDeliveryPort`：start / steer / cancel turn（`crates/agentdash-application-ports/src/runtime_session_delivery.rs:58`）。
- `AgentRunForkMaterializationPort`：创建 child run / agent / frame / anchor / lineage（`crates/agentdash-application-ports/src/agent_run_fork_materialization.rs:8`）。
- ContextFrame projector 保持 application 内部纯函数，不作为 repository。

## 删除清单

1. 删除 `AgentFrame` 上 in-place mutable visible refs。
   `visible_canvas_mount_ids_json` 和 `visible_workspace_module_refs_json` 当前在 frame row 上追加（`crates/agentdash-domain/src/workflow/agent_frame.rs:24`, `crates/agentdash-domain/src/workflow/agent_frame.rs:27`）。这些应改为 capability dimension / runtime exposure effect 后的新 revision。

2. 删除 `AgentFrameRepository::append_visible_*`。
   旧 revision 不应被修改。运行期 exposure 写新 frame revision，并通过 `RuntimeSurfaceAdoptionPort` 注入 live runtime。

3. 删除 `effective_capability_json + vfs_surface_json + mcp_surface_json` 的覆盖读模型。
   物理上可迁移成 `surface_json`；如保留列，只能作为由 `surface_json` 派生的只读投影。当前覆盖读取见 `project_capability_state_from_frame`（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:50`）。

4. 删除 `agent_frame_transitions` 作为 frame 命名的独立表。
   pending transition 是 runtime command payload，不是 AgentFrame revision。当前 `RuntimeCommandRecord` 同时嵌入 `RuntimeDeliveryCommand` 和 `AgentFrameTransitionRecord`（`crates/agentdash-spi/src/session_persistence.rs:390`），Postgres 又拆成 `agent_frame_transitions` + `session_runtime_commands` 两表（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:724`）。最小形态是一张 `session_runtime_commands` / mailbox wake row 记录 transition delta 与 delivery status。

5. 不新增 `ContextDeliveryRecord`。
   session event 已记录 ContextFrame trace；delivery plan 可由 turn frames 重算。新增 delivery 表会把 per-turn projection 误升为第二事实源。

6. 不恢复 `LifecycleAgent.current_frame_id`。
   current frame 由 revision 序列推导；delivery binding 只记录当前 runtime session 和 launch frame（`crates/agentdash-domain/src/workflow/lifecycle_agent.rs:127`）。

7. 替换 `created_by_id` 中塞 frame id 的弱类型 provenance。
   新增 `source_frame_id` 后，`created_by_id` 只表达 actor / command / grant id，不再承载 baseline frame。

## 迁移 / 实施顺序

1. 引入 `AgentFrameSurface` typed model 与 `agent_frames.surface_json`、`agent_frames.source_frame_id`。
   migration 用当前读法 `project_capability_state_from_frame` 回填 canonical surface：先按旧三列还原最终 `CapabilityState`，再写入 `surface_json`。预研阶段不需要兼容读取分支。

2. 改 `AgentFrameBuilder`。
   builder 只接受 typed `AgentFrameSurface` 或 capability transition replay 后的 `CapabilityState`，写新 revision；删除 carry-forward 旧三列和 visible refs 的隐式复制。

3. 改 runtime surface query / launch envelope。
   `FrameSurfaceDraft::from_frame` 只读 `surface_json`；`FrameLaunchSurface::new` 的 equality check 可以保留为构造期 invariant，但不再对应多个持久列。

4. 改 Canvas / workspace module runtime exposure。
   `apply_canvas_runtime_surface_update` 不再 append frame visible refs；它 replay VFS/workspace-module effects 得到新 `CapabilityState`，写新 frame revision，然后 adopt。

5. 收敛 permission grant effect。
   grant approval / revoke 若改变 capability surface，生成新 frame revision；grant row 继续作为 authorization/admission fact，active grant 查询以 current frame 为 effect frame。

6. 收敛 pending runtime transition 存储。
   把 `AgentFrameTransitionRecord` 从 frame 语义中移出，保留 `RuntimeCapabilityTransition` 作为 session command payload；删除 `agent_frame_transitions` 表或更名为单表 command payload。

7. 改 fork materialization。
   child initial frame 写 `source_frame_id = parent_current_frame_id`；必要时 `agent_run_lineages` 增加 `child_frame_id`。不要把 fork baseline 绑到 parent launch frame。

8. 固化 ContextFrame projector。
   ContextFrame id 使用 `(turn_id, kind, source_digest)` 级别稳定规则；`ContextDeliveryPlan` 只在 `TurnPreparer` 内生成。hook direct emit 如果需要模型可见，改为 mailbox envelope，等下一 turn preparation 投递。

9. 验证与删除旧列。
   migration 后跑 repository / launch / fork / runtime surface / context delivery / frontend generated contract 测试；确认没有读旧列后删除旧 columns、append methods 和双表 transition 代码。

## 需要验证的代码事实

- `AgentFrame` 现在同时存 effective capability、context、VFS、MCP、execution profile 和 visible refs（`crates/agentdash-domain/src/workflow/agent_frame.rs:10`）。
- `AgentFrame::new_revision` 默认创建新 id / revision / created_by，不 carry surface（`crates/agentdash-domain/src/workflow/agent_frame.rs:59`）。
- visible refs 当前有 append 方法，会修改旧 frame（`crates/agentdash-domain/src/workflow/agent_frame.rs:86`, `crates/agentdash-domain/src/workflow/agent_frame.rs:113`）。
- `AgentFrameRepository::get_current` 是 current projection（`crates/agentdash-domain/src/workflow/repository.rs:83`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:475`）。
- `agent_frames(agent_id, revision)` 已唯一（`crates/agentdash-infrastructure/migrations/0001_init.sql:1034`）。
- `RuntimeSessionExecutionAnchor` 明确说明 launch frame 不随 revision 覆盖（`crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:25`）。
- delivery selection 先校验 current delivery binding 和 anchor，再取 current frame；返回值同时包含 current_frame_id 和 launch_frame_id（`crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:144`, `crates/agentdash-application-agentrun/src/agent_run/delivery_runtime_selection.rs:243`）。
- runtime surface query 通过 runtime session anchor 找 run/agent，再取 current frame，而不是读 launch frame（`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:310`）。
- 当前 capability 投影存在 VFS/MCP 覆盖层（`crates/agentdash-application-agentrun/src/agent_run/runtime_capability.rs:42`）。
- `FrameLaunchSurface::new` 已把 VFS/MCP 与 CapabilityState 一致性作为 invariant（`crates/agentdash-application-agentrun/src/agent_run/frame/runtime_launch.rs:87`）。
- `FrameLaunchEnvelope` 已按 frame / command / runtime / context / diagnostics 分组，适合作为 turn preparation 唯一输入（`crates/agentdash-application-ports/src/frame_launch_envelope.rs:111`）。
- `TurnPreparer` 是 ContextFrame 正式组装点，包含 startup context、guidelines、memory、owner bootstrap、pending transition、hook queue（`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:169`, `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:313`）。
- `ContextDeliveryPlan` 由 frames 排序生成，不是读取持久表（`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:589`）。
- `TurnCommitter` 在 accepted boundary 后 emit context frames（`crates/agentdash-application-runtime-session/src/session/launch/commit.rs:56`）。
- ContextFrame 持久化为 session event `context_frame`（`crates/agentdash-application-runtime-session/src/session/eventing.rs:367`）。
- PiAgent system prompt 只消费 delivery metadata 标为 system/developer 且 consume 的 frames（`crates/agentdash-executor/src/connectors/pi_agent/connector.rs:1157`）。
- generic context renderer 会把所有 ContextFrame 文本拼入 prompt（`crates/agentdash-executor/src/connectors/context_frame_render.rs:3`），因此 delivery metadata 是更正确的消费边界。
- hook runtime turn-start notice queue 有去重、上限和 drain（`crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:492`, `crates/agentdash-application-agentrun/src/agent_run/frame/hook_runtime.rs:511`）。
- fork resolve parent 使用 current delivery selection，再读取 selection.current_frame_id（`crates/agentdash-application-agentrun/src/agent_run/fork.rs:538`）。
- fork materialization 复制 parent frame surface 到 child revision 1，但当前没有 typed baseline 字段（`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:130`）。
- `AgentRunLineage` 当前记录 parent/child run/agent/runtime session 和 fork point，不记录 frame baseline（`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:12`）。
- `AgentFrameTransitionRecord` 目前是 pending transition delta，不是完整 surface（`crates/agentdash-spi/src/session_persistence.rs:59`）。
- `RuntimeDeliveryCommandKind` 只有 `PendingRuntimeContext`，说明它是 command/outbox 语义（`crates/agentdash-spi/src/session_persistence.rs:412`）。
- Postgres runtime command upsert 同时写 `agent_frame_transitions` 和 `session_runtime_commands`，这是可合并的双表投递模型（`crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:680`）。
