# 设计

## Architecture Goal

目标不是继续给 `start_draft`、`send_next`、`enqueue`、`steer`、pending queue、resource browser 各自补判断，而是建立一套 AgentRun conversation control model：

1. 后端用同一个 resolver 读取 run / agent / frame / runtime session / active turn / pending queue / model config / resource surface。
2. resolver 生成一份 `AgentConversationSnapshot`。
3. 前端只渲染 snapshot，不再本地解释业务状态。
4. 每次提交都带明确 `ConversationCommandIntent`，并由后端用 snapshot precondition 校验。
5. 模型配置和 resource surface 都是 snapshot 的一等字段。

## Current State

```mermaid
flowchart TD
  DraftRoute["/agent-runs/new"] --> AgentPage["AgentRunWorkspacePage"]
  AgentPage --> ExecHydrate["SessionChatView local executor hydrate"]
  ExecHydrate --> LocalStorage["localStorage executor config"]
  ExecHydrate --> AgentDefaults["ProjectAgent executor defaults"]
  ExecHydrate --> FrameProfile["frame execution_profile"]
  ExecHydrate --> Hint["executor hint"]
  ExecHydrate --> Discovery["discovered options default_model"]
  Discovery -. "not authoritative write-back" .-> ExecConfig["executorConfig sent by UI"]
  AgentDefaults -. "may arrive async" .-> ExecConfig
  LocalStorage --> ExecConfig

  ExecConfig --> StartReq["POST /projects/{project}/agents/{agent}/agent-runs"]
  StartReq --> StartSvc["ProjectAgentRunStartService"]
  StartSvc --> Dispatch["LifecycleDispatchService materializes run/agent/frame/session"]
  Dispatch --> Bind["bind LifecycleAgent.project_agent_id"]
  Bind --> MessageSvc["AgentRunMessageService initial message"]
  MessageSvc --> FrameConstruction["FrameConstructionService"]
  FrameConstruction --> Merge["merge_user_executor_config"]
  Merge --> Connector["connector delivery"]
  Merge -. "executor-only user config can replace preset provider/model" .-> MissingModel["missing model/provider failure"]
  MissingModel --> Toast["UI shows create ProjectAgent AgentRun failed / send error"]

  RunRoute["/agent-runs/:runId/:agentId"] --> WorkspaceFetch["GET AgentRunWorkspaceView"]
  WorkspaceFetch --> Actions["actions send_next/enqueue/steer/cancel"]
  WorkspaceFetch --> Control["control_plane status"]
  WorkspaceFetch --> PendingQueue["pending_queue paused"]
  WorkspaceFetch --> FrameRuntime["frame_runtime"]
  WorkspaceFetch --> DeliveryRef["delivery_runtime_ref"]

  AgentPage --> ChatControl["deriveAgentRunWorkspaceChatControlState"]
  Control --> ChatControl
  Actions --> ChatControl
  ChatControl --> Composer["SessionChatView composer"]
  Composer --> Keydown["Enter/Ctrl+Enter local keydown"]
  Keydown -->|primaryAction enqueue + secondary steer| SteerReq["POST /steering with last_turn_id"]
  Keydown -->|primary action| SendOrEnqueue["POST /messages or /pending-messages"]

  SteerReq --> ExpectedTurn["API expected_turn_id check"]
  ExpectedTurn --> Mismatch["expected_turn_id mismatch when snapshot/key state stale"]
  SendOrEnqueue --> PendingConflict["pending enqueue conflict when session completed/idle"]

  PendingQueue --> PendingBanner["PendingMessageList displays paused even with no messages"]

  DeliveryRef --> ResolveSessionSurface["resolveVfsSurface(session_runtime)"]
  FrameRuntime -. "stored but not resource source" .-> WorkspacePanel["workspace panel/resource browser"]
  ResolveSessionSurface --> WorkspacePanel
  FrameConstruction --> LifecycleMount["lifecycle mount in AgentFrame vfs_surface_json"]
  LifecycleMount -. "not guaranteed visible through frontend path" .-> WorkspacePanel
```

### Current Entry Inventory

| Entry | Current backend path | Current frontend path | Primary risk |
| --- | --- | --- | --- |
| ProjectAgent draft start | `project_agents.rs -> ProjectAgentRunStartService -> AgentRunMessageService` | `AgentRunWorkspacePage -> createProjectAgentRun` | executor-only config drops preset model/provider |
| Send next | `lifecycle_agents.rs /messages -> ensure_send_next_allowed` | primary action `send_next` | readiness split across workspace status and execution state |
| Enqueue | `/pending-messages -> ensure_pending_enqueue_allowed` | primary action `enqueue` when running | stale running state tries enqueue after completed/idle |
| Steer | `/steering -> expected_turn_id -> AgentRunSteeringService` | Ctrl/Cmd+Enter secondary action | stale turn id or idle state becomes steer |
| Promote pending | `/pending-messages/{id}/promote` | pending row action | depends on running active turn |
| Resume pending | `/pending-messages/resume` | pending queue banner action | pause visible even without visible pending work |
| Cancel | `/cancel` | cancel action | cancelling and terminal cleanup not part of one command state machine |
| Workspace resources | `session_runtime -> AgentFrame vfs` resolver | `resolveVfsSurface(session_runtime)` | AgentRun workspace and panel consume different resource facts |

## Target State

```mermaid
flowchart TD
  subgraph Facts["Control Facts"]
    Run["LifecycleRun"]
    Agent["LifecycleAgent"]
    Frame["current AgentFrame"]
    Anchor["RuntimeSessionExecutionAnchor"]
    Runtime["RuntimeSession execution state"]
    Turn["active turn / last terminal turn"]
    Pending["PendingQueue state + visible messages"]
    ProjectAgent["ProjectAgent preset"]
    Discovery["Executor discovery"]
  end

  Facts --> Resolver["AgentConversationSnapshotResolver"]

  Resolver --> ModelResolver["ModelConfigResolver"]
  Resolver --> CommandResolver["CommandIntentResolver"]
  Resolver --> ResourceResolver["ResourceSurfaceResolver"]
  Resolver --> LifecycleResolver["LifecycleContextResolver"]

  ModelResolver --> ModelState["model_config: resolved | model_required"]
  CommandResolver --> Commands["commands: start_draft/send_next/enqueue/steer/promote/resume/cancel"]
  ResourceResolver --> Surface["resource_surface: AgentFrame VFS + lifecycle mount + browsing policy"]
  LifecycleResolver --> Context["run/agent/frame/runtime/subject refs"]

  ModelState --> Snapshot["AgentConversationSnapshot"]
  Commands --> Snapshot
  Surface --> Snapshot
  Context --> Snapshot
  Pending --> Snapshot

  Snapshot --> FEPage["AgentRunWorkspacePage"]
  FEPage --> Composer["SessionChatComposer"]
  FEPage --> ModelSelector["Model selector"]
  FEPage --> PendingUI["Pending queue UI"]
  FEPage --> WorkspacePanel["Workspace panel/resource browser"]

  Composer --> Intent["ConversationCommandIntent"]
  ModelSelector --> ModelOverride["explicit model override"]
  PendingUI --> PendingIntent["resume/promote/delete intent"]
  Intent --> CommandAPI["single command handler or shared precondition layer"]
  ModelOverride --> CommandAPI
  PendingIntent --> CommandAPI
  CommandAPI --> Resolver
```

## Target Snapshot Contract

`AgentConversationSnapshot` can be implemented by extending `AgentRunWorkspaceView` first, but the contract should be modeled as a full conversation snapshot:

```rust
pub struct AgentConversationSnapshot {
    pub identity: AgentConversationIdentity,
    pub lifecycle_context: AgentConversationLifecycleContext,
    pub execution: ConversationExecutionView,
    pub model_config: ConversationModelConfigView,
    pub commands: ConversationCommandSetView,
    pub pending: ConversationPendingQueueView,
    pub resource_surface: ConversationResourceSurfaceView,
    pub diagnostics: Vec<ConversationDiagnosticView>,
}
```

### Command View

Each user-visible command should be projected as data rather than inferred by components:

```rust
pub struct ConversationCommandView {
    pub kind: ConversationCommandKind,
    pub enabled: bool,
    pub unavailable_reason: Option<String>,
    pub placement: ConversationCommandPlacement,
    pub shortcut: Option<ConversationShortcut>,
    pub requires_input: bool,
    pub executor_config_policy: ExecutorConfigPolicy,
    pub precondition: ConversationCommandPrecondition,
}
```

The frontend may map labels/icons locally, but `kind`, `enabled`, `shortcut`, `executor_config_policy`, and `precondition` come from the snapshot adapter.

### Model Config

```mermaid
flowchart LR
  Preset["ProjectAgent preset AgentConfig"] --> Merge["field-level merge"]
  Frame["current frame execution_profile"] --> Merge
  User["explicit user override"] --> Merge
  Discovery["executor discovery default_model"] --> Resolve["model requirement resolver"]
  Merge --> Resolve
  Resolve -->|complete| Resolved["resolved executor/provider/model"]
  Resolve -->|missing required fields| Required["model_required command state"]
```

Rules:

- Preset and frame defaults are authoritative persisted defaults.
- User override is field-level: executor/provider/model/thinking/permission replace only the fields supplied by the user.
- An executor-only override keeps preset provider/model when they are still valid for that executor.
- Discovery `default_model` may fill a missing model only when the backend marks it as valid for the selected executor/provider.
- If the selected executor requires explicit model selection and no valid model exists, snapshot enters `model_required` and command submission is disabled with a precise reason.
- ProjectAgent summary exposes `effective_executor_config` with `source` and `validity`; localStorage may provide recent choices, but not a ProjectAgent default.
- Command stores propagate API errors instead of converting command failure into `null`.

## Command State Machine

```mermaid
stateDiagram-v2
  [*] --> Draft
  Draft --> ModelRequired: no resolved model
  Draft --> ReadyToStart: model resolved
  ModelRequired --> ReadyToStart: user selects model
  ReadyToStart --> StartingClaimed: start_draft accepted
  StartingClaimed --> RunningActive: turn activated
  StartingClaimed --> Failed: launch/model/surface/delivery failed
  RunningActive --> RunningActive: enqueue accepted
  RunningActive --> RunningActive: steer accepted with active_turn guard
  RunningActive --> Cancelling: cancel accepted
  RunningActive --> Ready: turn completed
  RunningActive --> ReadyWithPausedQueue: turn failed/interrupted and visible pending work exists
  ReadyWithPausedQueue --> Ready: resume/delete drains pending work
  ReadyWithPausedQueue --> StartingClaimed: auto-drain pending after completed turn
  Ready --> StartingClaimed: send_next accepted
  Cancelling --> Ready: executor stops without terminal agent
  Cancelling --> Terminal: agent/run terminal
  Ready --> Terminal: agent/run terminal
  Failed --> Ready: recoverable command surface exists
```

`StartingClaimed` maps to the existing session `TurnState::Claimed`: the session is reserved, but there is not yet an active turn that can accept steer/promote. `RunningActive` maps to `TurnState::Active(turn_id)` and is the only state that may expose steer or promote-to-steer.

### Keyboard Mapping

| Snapshot command mode | Enter | Ctrl/Cmd+Enter | Notes |
| --- | --- | --- | --- |
| `draft.ready_to_start` | `start_draft` | `start_draft` | model must be resolved |
| `model_required` | none | none | selector focused/displayed |
| `starting_claimed` | none | none | no active turn for steer/promote |
| `ready` | `send_next` | `send_next` | never steer when not running |
| `running_active.enqueue_only` | `enqueue` | `enqueue` | no hidden steer |
| `running_active.enqueue_and_steer` | `enqueue` | `steer` | steer command carries snapshot active turn token |
| `cancelling` | none | none | cancel/stop controls only |
| `terminal` | none | none | readonly |

The frontend should receive this mapping from snapshot, not reconstruct it from `primaryAction.kind`.

## Pending Queue Projection

```rust
pub struct ConversationPendingQueueView {
    pub visible_messages: Vec<PendingMessageView>,
    pub paused: bool,
    pub user_attention: bool,
    pub resume_command: Option<ConversationCommandAvailabilityView>,
    pub message: Option<String>,
}
```

Rules:

- `paused` records queue mechanics.
- `visible_messages` records user-visible queued work.
- `user_attention` is true only when the UI should render a banner.
- A terminal or stopped session with no visible pending messages does not show a pending banner.
- Resume is a command availability, not a direct function of paused.

## Resource Surface Projection

```mermaid
flowchart TD
  Frame["current AgentFrame"] --> FrameVfs["typed vfs_surface_json"]
  ActiveWorkflow["active workflow/lifecycle projection"] --> LifecycleMount["lifecycle_vfs mount"]
  FrameVfs --> ResourceResolver["ConversationResourceSurfaceResolver"]
  LifecycleMount --> ResourceResolver
  ResourceResolver --> SurfaceView["resource_surface in snapshot"]
  SurfaceView --> Agent["connector/session launch"]
  SurfaceView --> Frontend["workspace panel/resource browser"]
```

Rules:

- Agent execution and frontend workspace panel consume the same resource surface projection.
- `session_runtime` may remain a lookup key, but it is not the frontend's resource truth source.
- ProjectAgent explicit lifecycle uses ProjectAgent owner surface plus lifecycle mount.
- Workflow AgentCall uses node-scoped lifecycle surface plus node mount policy.
- The resolver validates three facts together: active workflow projection, persisted `AgentFrame.vfs_surface_json`, and resolved `ResolvedVfsSurface`. If active workflow exists but `lifecycle_vfs` is absent from persisted/resolved surface, the snapshot reports a resource diagnostic.
- For a delivery runtime session, `session_runtime` VFS resolution should use the delivery/accepted frame for that session, not an ambiguous `current_frame.or(anchor_frame)` choice unless the resolver proves they are the same resource surface.

## Implementation Shape

1. Add resolvers as application/domain services while keeping public endpoints in place.
2. Extend generated contracts to carry model state, command modes, pending user attention, and resource surface.
3. Route every command endpoint through the shared resolver/precondition checker.
4. Refactor frontend composer/model selector/pending/resource browser to consume snapshot.
5. Remove duplicate local inference after tests prove all entry points use snapshot.

## Misleading Path Eradication

The target architecture is valid only if old paths cannot silently keep shaping future code. Every path that still advertises a competing owner must be classified and removed or renamed as part of the implementation.

```mermaid
flowchart TD
  Inventory["Search and inventory old paths"] --> Classify{"Path role"}
  Classify -->|command/control| Kill["Delete or route through ConversationCommandIntent"]
  Classify -->|resource surface| Surface["Route through snapshot resource_surface"]
  Classify -->|trace/diagnostic| Rename["Rename and document as trace-only"]
  Classify -->|generated type/test residue| Regenerate["Regenerate contracts and update tests"]

  Kill --> Gate["grep/audit gate"]
  Surface --> Gate
  Rename --> Gate
  Regenerate --> Gate
  Gate --> Clean["No misleading public or semi-public path remains"]
```

Cleanup rules:

- Session routes may remain for trace, events, approvals, context audit, lineage and terminal inspection, but not as user-facing AgentRun command control.
- `SessionRuntimeControlView` and `SessionRuntimeActionSetView` either disappear from interactive frontend consumption or are renamed/scoped as runtime diagnostics.
- ProjectAgent `/launch` and `ProjectAgentLaunchResult` must be removed or made an internal materialization helper that cannot be mistaken for the primary run start path.
- Frontend `primaryAction/secondaryAction` types must stop representing business command semantics after command list adoption.
- `useAgentRunWorkspaceState` must not call `resolveVfsSurface(session_runtime)` as the workspace panel source after snapshot `resource_surface` exists.
- Store actions for command APIs must not return `null` on failure; old null-return contracts should be removed from callers and tests.
- Tests that assert stale action bits, terminal enqueue/steer behavior, or session-runtime resource control must be rewritten around snapshot commands and diagnostics.
- Generated contracts must not continue exporting interactive control DTOs whose names imply RuntimeSession owns AgentRun commands.

## Review Question

唯一需要用户确认的产品命名决策：URL 和侧边栏是否继续叫 AgentRun。推荐保留 `AgentRun` 作为产品 identity，内部 contract 使用 conversation snapshot 来表达完整会话状态。
