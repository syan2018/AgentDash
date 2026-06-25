# Design: Canvas 交互诊断与模块边界预研

## Architecture Summary

本任务规划四条边界：

1. Canvas render observation：前端 iframe/父页面采集当前真实渲染状态，后端保存 AgentRun↔Canvas 引用上的 latest observation，Agent 通过 workspace module/tool 查询。
2. Canvas interaction state：Canvas source 通过显式 SDK 在 AgentRun↔Canvas 引用上上报用户表单、选区、过滤器等 Agent 可见状态；状态默认作为可查询事实，只有提交时才进入模型输入。
3. Canvas submit-to-Agent：Canvas 内用户动作构造 canonical `UserInputBlock`，后端解析 AgentRun↔Canvas 引用并复用 AgentRun Mailbox 投递。
4. Canvas / Workspace Module crate boundary：评估将 Canvas 与 Workspace Module 从当前 application/domain/api 分散路径中收束为独立 crate 或 crate family。

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
  runtime_session_id?: string;
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
  -> frontend service uploads latest observation
  -> backend resolves AgentRun Canvas reference and stores latest observation by run_id + agent_id + canvas_mount_id
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
  runtime_session_id?: string;
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
POST /canvases/{canvas_id}/agent-input-submit
{
  "run_id": "...",
  "agent_id": "...",
  "canvas_mount_id": "...",
  "runtime_session_id": "...",
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
  -> load Canvas with project edit/use permission
  -> validate Canvas is visible through the target AgentRun Canvas reference
  -> resolve current AgentRun delivery target
  -> optionally validate runtime_session_id matches current delivery runtime when provided
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
runtime_session_id, frame_id,
payload_json, artifact_ref, created_at, updated_at
```

Reasons:

- Agent tools and submit payloads can reference stable ids.
- Restart and scheduler delays preserve the facts used by Agent.
- Future cleanup can follow session/runtime retention.
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

Use a small crate family rather than one oversized crate:

| Crate | Responsibility |
| --- | --- |
| `agentdash-canvas` | Canvas domain entity, value objects, runtime snapshot/resource service contracts, Canvas-specific operation keys. |
| `agentdash-workspace-module` | Workspace module identity, presentation URI, module descriptors, operation invocation contracts. |
| `agentdash-canvas-application` or application submodule | Use cases that require repositories, RuntimeGateway, VFS service, AgentRun surface update, extension packaging. |

Alternative: start with `agentdash-canvas` only, then extract `agentdash-workspace-module` after module API stabilizes.

### Dependency Direction

Recommended direction:

```text
agentdash-domain / agentdash-spi
  -> agentdash-canvas
  -> agentdash-application
  -> agentdash-api
```

Workspace module contracts should be usable by Canvas and extension runtime without depending on API route implementations.

### Split Boundaries

Good candidates for `agentdash-canvas`:

- Canvas identity helpers: `canvas_mount_id`, `canvas_vfs_mount_id`, module id, presentation URI.
- Canvas value objects and validation.
- Runtime snapshot model and binding/resource descriptor types.
- Browser bridge action key constants and operation keys.
- Canvas embedded skill bundle path metadata if it remains Canvas-owned.

Keep in application layer:

- Repository implementations.
- RuntimeGateway invocation.
- AgentRun runtime surface update and adoption.
- VFS service reads/writes.
- Extension package artifact storage.
- HTTP authorization and route mapping.

Workspace Module candidate extraction:

- `workspace_module_presented` event payload model.
- Module id and presentation URI helpers.
- Operation descriptor model and invocation result envelope.
- Cross-module operation key registry.

### Migration Strategy

1. Extract pure identity/value/helper code first.
2. Move runtime snapshot/resource types after contracts are stable.
3. Move workspace module descriptor/operation contracts after Canvas bridge API design is accepted.
4. Keep application services in `agentdash-application` until runtime surface update and VFS dependencies are cleaner.
5. Regenerate contracts and run drift checks after DTO moves.

## Proposed Task Split

Parent task: this planning task owns the overall requirements and architecture decision.

Recommended children:

1. Canvas runtime observation and interaction state MVP.
2. Canvas submit-to-Agent mailbox bridge.
3. Canvas / Workspace Module crate boundary extraction design and first pure-helper extraction.
4. Integration review: extension canvas panel, runtime surface, VFS asset/binding and AgentRun workspace behavior.

## Risks

- Canvas actual render state exists only in the browser; backend-owned diagnosis must be fed by frontend observation.
- Pixel screenshot capture can fail or become tainted when Canvas loads external images or canvas elements.
- AgentRun Mailbox source enum and generated contracts will change; all consumer rows and tests must be updated together.
- Crate extraction may collide with active runtime surface and Canvas VFS convergence tasks; sequencing must avoid moving files while another task edits behavior in the same modules.
- Extension `canvas_panel` currently reuses packaged snapshots and may need live session hydration before new bridge features work there.

## Validation Strategy

- Frontend unit tests for preview bridge envelopes and SDK shape.
- Backend contract tests for Canvas submit DTOs and mailbox source generation.
- Backend application tests for session-to-AgentRun resolution and mailbox acceptance.
- API tests for Canvas project/session mismatch, missing interaction snapshot, duplicate client command id and steer/queue delivery intent.
- Frontend integration tests for Canvas button submit outcome and latest observation upload.
- Crate boundary checks through `cargo check`, contract generation, and targeted dependency graph inspection.
