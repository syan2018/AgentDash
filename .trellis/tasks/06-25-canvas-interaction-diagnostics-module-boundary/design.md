# Design: Canvas 交互诊断与模块边界实现

## Architecture Summary

本任务实现四条边界：

1. Canvas render observation：前端 iframe/父页面采集当前真实渲染状态，后端保存 AgentRun↔Canvas 引用上的 latest observation，Agent 通过 workspace module/tool 查询。
2. Canvas interaction state：Canvas source 通过显式 SDK 在 AgentRun↔Canvas 引用上上报用户表单、选区、过滤器等 Agent 可见状态；状态默认作为可查询事实，只有提交时才进入模型输入。
3. Canvas submit-to-Agent：Canvas 内用户动作构造 canonical `UserInputBlock`，后端解析 AgentRun↔Canvas 引用并复用 AgentRun Mailbox 投递。
4. Canvas / Workspace Module crate boundary：将 Workspace Module 业务从 application 收束到 `agentdash-workspace-module`，Canvas 作为其子模块；Canvas 领域实体和 repository/runtime state trait 保留在 `agentdash-domain::canvas`。

## Current Code Anchors

- Frontend runtime preview：`packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx`
- Preview boot SDK：`packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts`
- Canvas API routes：`crates/agentdash-api/src/routes/canvases.rs`
- Canvas application modules：`crates/agentdash-application/src/canvas/*`
- AgentRun mailbox service：`crates/agentdash-application/src/agent_run/mailbox.rs`
- AgentRun mailbox contracts：`crates/agentdash-contracts/src/agent/run_mailbox.rs`
- Runtime surface update：`crates/agentdash-application/src/agent_run/runtime_surface_update.rs`
- Extension canvas reuse：`packages/app-web/src/features/extension-runtime/ui/ExtensionCanvasPanel.tsx`

## Channel 1: Render Observation

### Goal

让 Agent 查询用户当前看到的 Canvas 运行状态，而不是只读取 Canvas source 或 runtime snapshot。

### Proposed Shape

```ts
type CanvasRuntimeObservation = {
  observation_id: string;
  run_id: string;
  agent_id: string;
  agent_run_canvas_ref: string;
  canvas_id: string;
  canvas_mount_id: string;
  delivery_trace_ref?: string;
  frame_id: string;
  generation: number;
  captured_at: string;
  status: "building" | "ready" | "error";
  message?: string;
  viewport: {
    width: number;
    height: number;
    device_pixel_ratio: number;
  };
  document: {
    root_empty: boolean;
    body_text_preview: string;
    element_count: number;
    focused_element?: string;
  };
  diagnostics: Array<{
    level: "info" | "warn" | "error";
    source: "runtime" | "console" | "bridge";
    message: string;
  }>;
  screenshot_ref?: string;
};
```

### Data Flow

```text
Canvas iframe runtime
  -> canvas-render-observation postMessage
  -> CanvasRuntimePreview validates frame_id/generation
  -> frontend service uploads latest observation through AgentRun-scoped Canvas route
  -> backend resolves AgentRun Canvas reference and current delivery trace, then stores latest observation by run_id + agent_id + canvas_mount_id
  -> Agent tool canvas.inspect_render_state returns observation
```

### Capture Semantics

- Passive capture occurs on preview ready/error, runtime error, and explicit refresh.
- On-demand capture can be added by sending a parent-to-iframe `canvas-inspect-request` message and waiting for a bounded response.
- Screenshot is an optional artifact reference. DOM/diagnostic observation is the MVP because it is smaller and more reliable than pixel capture.

## Channel 2: Interaction State

### Goal

让 Canvas source 显式声明哪些用户交互状态对 Agent 有意义。

### Proposed Browser SDK

```ts
window.agentdash.interaction.setState(key, value, options?)
window.agentdash.interaction.clearState(key)
window.agentdash.interaction.emit(event)
window.agentdash.interaction.getState()
```

Example:

```ts
await window.agentdash.interaction.setState("selection", {
  kind: "table_row",
  ids: ["row-17"],
  summary: "华东区域 Q2 数据",
});
```

### Proposed Stored Shape

```ts
type CanvasInteractionSnapshot = {
  snapshot_id: string;
  run_id: string;
  agent_id: string;
  agent_run_canvas_ref: string;
  canvas_id: string;
  canvas_mount_id: string;
  delivery_trace_ref?: string;
  frame_id: string;
  updated_at: string;
  state: Record<string, unknown>;
  recent_events: Array<{
    kind: string;
    payload: unknown;
    occurred_at: string;
  }>;
};
```

### Agent Consumption

- `canvas.get_interaction_state` returns latest snapshot for query/diagnosis.
- Canvas submit-to-Agent may include snapshot summary or snapshot id in the mailbox payload.
- Interaction state is not automatically appended to model-visible history.

## Channel 3: Canvas Submit-To-Agent

### Goal

Canvas 内按钮、表单提交或 selection action 可以构造请求并输入给当前关联 Agent。

### Proposed Browser SDK

```ts
window.agentdash.agent.submit({
  text,
  input,
  include_interaction_state,
  include_render_observation,
  delivery_intent,
  client_command_id,
})
```

### Recommended Backend Route

```text
POST /agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/agent-input-submit
{
  "client_command_id": "...",
  "input": [
    { "type": "text", "text": "...", "text_elements": [] }
  ],
  "delivery_intent": "queue" | "steer",
  "interaction_snapshot_id": "...",
  "render_observation_id": "..."
}
```

### Backend Flow

```text
Canvas submit route
  -> resolve AgentRun + Agent + current Canvas reference from path
  -> load Canvas through the AgentRun project and mount id
  -> validate Canvas is visible through the target AgentRun Canvas reference
  -> resolve current AgentRun delivery target and trace ref on the backend
  -> apply AgentRun command policy for Canvas-origin submit
  -> AgentRunMailboxService.accept_user_message(
       source = CanvasAction,
       input = canonical UserInputBlock[],
       delivery_intent = queue/steer
     )
  -> return AgentRunMessageCommandResponse
```

### Contract Changes

- Add request/response DTOs under Canvas or AgentRun mailbox contracts.
- Add `MailboxMessageSource::CanvasAction` or equivalent.
- Regenerate TypeScript contracts after Rust DTO changes.

## Boundary With Runtime Actions

`window.agentdash.invoke(action_key, input)` remains for runtime actions such as MCP/tool invocation. Canvas submit-to-Agent represents user input and should enter AgentRun Mailbox. Both are explicit user-triggered Canvas actions, but they produce different facts:

| Canvas API | Fact produced | Backend owner |
| --- | --- | --- |
| `agentdash.invoke` | RuntimeInvocationResult / tool action | RuntimeGateway |
| `agentdash.agent.submit` | UserInputBlock mailbox message | AgentRunMailboxService |
| `agentdash.interaction.setState` | Latest interaction snapshot | Canvas runtime state service |
| `agentdash.inspect/capture` | Render observation | Canvas runtime state service |

## Storage Evaluation

### Option A: In-Memory Latest State

- Smallest implementation.
- Loses state on backend restart.
- Weak fit for Agent tool queries and audit.

### Option B: AgentRun Canvas Reference Latest Tables

Recommended.

Candidate tables:

```text
canvas_runtime_observations
canvas_interaction_snapshots
```

Fields:

```text
id, run_id, agent_id, canvas_id, canvas_mount_id, agent_run_canvas_ref,
delivery_trace_ref, frame_id,
payload_json, artifact_ref, created_at, updated_at
```

Reasons:

- Agent tools and submit payloads can reference stable ids.
- Restart and scheduler delays preserve the facts used by Agent.
- Future cleanup can follow AgentRun workspace retention.
- The ownership follows the AgentRun-visible Canvas reference rather than the RuntimeSession substrate.

## Canvas / Workspace Module Crate Boundary Evaluation

### Current Shape

Canvas and Workspace Module responsibilities are spread across:

- `agentdash-domain`: Canvas entity/repository/value objects and embedded canvas skill docs.
- `agentdash-application`: Canvas management/runtime/resource/promotion/tool helpers.
- `agentdash-api`: Canvas HTTP routes and DTO mapping.
- `agentdash-contracts`: generated Canvas/extension/mailbox contracts.
- Frontend feature modules under `packages/app-web`.

Workspace Module concepts appear in Canvas presentation, extension runtime panel registration, runtime surface update, and workspace-panel tab composition.

### Recommended Target

Use `agentdash-workspace-module` as the business crate for Workspace Module concepts and Canvas as its submodule:

| Crate | Responsibility |
| --- | --- |
| `agentdash-domain::canvas` | Canvas entity, value objects, repository traits, runtime observation / interaction snapshot contracts, and embedded Canvas skill bundle. |
| `agentdash-workspace-module` | Workspace module identity, presentation URI, module descriptors, operation invocation contracts, runtime tool provider, and Canvas submodule business services. |
| `agentdash-workspace-module::canvas` | Canvas mount/module/presentation identity helpers, management/runtime/VFS/visibility business logic, operation keys, runtime resource helpers, and repository-set use case boundary over domain traits. |
| `agentdash-application` submodules | Thin facades/adapters and cross-aggregate use cases that require AgentRun delivery selection, extension package artifact storage, or API/application composition. |

### Dependency Direction

Dependency direction:

```text
agentdash-domain
  <- agentdash-workspace-module
  <- agentdash-application
  <- agentdash-api

agentdash-infrastructure -> agentdash-domain
```

Workspace module contracts should be usable by Canvas and extension runtime without depending on API route implementations. Infrastructure implements `agentdash-domain::canvas` traits and must not depend on `agentdash-workspace-module`.

Workspace Module runtime tools call an AgentRun-facing bridge, not a session-facing bridge. The bridge resolves current AgentRun delivery runtime surface, exposes Canvas mounts, and injects AgentRun notifications. Runtime session ids may appear inside the API/application adapter as delivery trace coordinates, but they are not Canvas-facing contract fields and should not name the workspace-module business abstraction.

### Split Boundaries

Move into `agentdash-workspace-module::canvas`:

- Canvas identity helpers: `canvas_mount_id`, `canvas_vfs_mount_id`, module id, presentation URI.
- Canvas management, mutation validation, runtime snapshot/resource helpers, VFS mount/provider helpers and visibility rules that operate over domain Canvas types.
- Browser bridge action key constants and operation keys.

Keep in `agentdash-domain::canvas`:

- Canvas entity and value objects.
- Canvas repository traits and runtime state repository trait.
- Runtime observation / interaction snapshot contract structs.
- Canvas embedded skill bundle metadata.

Keep in application/API/infrastructure layers:

- Repository implementations.
- AgentRun delivery selection and runtime surface update/adoption.
- VFS service reads/writes.
- Extension package artifact storage.
- HTTP authorization and route mapping.

Workspace Module top-level extraction:

- `workspace_module_presented` event payload model.
- Module id and presentation URI helpers.
- Operation descriptor model and invocation result envelope.
- Cross-module operation key registry.

### Migration Strategy

1. Create `agentdash-workspace-module` and move Workspace Module / Canvas business services out of `agentdash-application`.
2. Keep Canvas entity/value/repository/runtime state contracts in `agentdash-domain::canvas`.
3. Move Workspace Module identity/descriptor/operation contracts and runtime tool provider into the workspace-module crate.
4. Connect workspace-module runtime tools to application through `WorkspaceModuleAgentRunBridge`, not a session bridge.
5. Keep HTTP authorization, API route mapping, Postgres adapters, concrete RuntimeGateway/service wiring, AgentRun delivery selection, and extension package artifact storage in the application/API/infrastructure layers.
6. Regenerate contracts and run drift checks after DTO moves.

## Implementation Scope

本任务直接实现 Canvas runtime observation、interaction state、submit-to-Agent 与 Workspace Module 业务 crate 边界收束。Canvas 作为 `agentdash-workspace-module::canvas` 子模块承载业务服务；Canvas 领域实体和 repository/runtime state trait 留在 `agentdash-domain::canvas`，保证 infrastructure 不反向依赖 workspace-module。

## Risks

- Canvas actual render state exists only in the browser; backend-owned diagnosis must be fed by frontend observation.
- Pixel screenshot capture can fail or become tainted when Canvas loads external images or canvas elements.
- AgentRun Mailbox source enum and generated contracts will change; all consumer rows and tests must be updated together.
- Crate extraction may collide with active runtime surface and Canvas VFS convergence tasks; sequencing must avoid moving files while another task edits behavior in the same modules.
- Extension `canvas_panel` currently reuses packaged snapshots and may need live AgentRun bridge hydration before new bridge features work there.

## Validation Strategy

- Frontend unit tests for preview bridge envelopes and SDK shape.
- Backend contract tests for Canvas submit DTOs and mailbox source generation.
- Backend application tests for AgentRun-to-Canvas reference resolution and mailbox acceptance.
- API tests for Canvas project/run mismatch, missing interaction snapshot, duplicate client command id and steer/queue delivery intent.
- Frontend integration tests for Canvas button submit outcome and latest observation upload.
- Crate boundary checks through `cargo check`, contract generation, and targeted dependency graph inspection.
