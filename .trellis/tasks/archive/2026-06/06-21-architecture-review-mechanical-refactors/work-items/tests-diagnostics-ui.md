# Tests / Diagnostics / UI Items

## M14 固化 runtime status aggregation owner tests

- Scope: workflow runtime reducer、domain `LifecycleRunStatus` aggregation tests。
- Acceptance: tests 覆盖 failed、blocked、running、ready、cancelled/completed、append orchestration；spec owner 不在本 item 中修改。
- Validation: `cargo test -p agentdash-domain workflow`, `cargo test -p agentdash-application workflow`.

## M15 top-level `AgentRunWorkspaceView.control_plane` display-only 验证

- Scope: API mapper、frontend command consumption。
- Acceptance: command enablement 只来自 `conversation.commands`；top-level `control_plane` 只用于粗粒度 display status。
- Validation: frontend tests/typecheck plus `rg "control_plane" packages/app-web/src`.

## M16 WorkspaceModule runtime deps 缺失可观测诊断

- Scope: `workspace_module/runtime_tool_provider`, session tool assembly diagnostics。
- Acceptance: 缺 RuntimeGateway/channel transport 时不是纯 warn-only，诊断能在 session/tool assembly 中被观察。
- Validation: targeted backend tests or diagnostics assertions.

## M17 workspace routing 文案区分 binding 与 execution

- Scope: workspace routing helper、settings/runtime summary UI 文案。
- Acceptance: Workspace routing 只表达目录 binding/readiness；执行忙闲只从 runtime summary 表达。
- Validation: `pnpm run frontend:check`.

## M18 Profile UI 将 machine id 表达为只读事实

- Scope: `packages/views/src/local-runtime/LocalRuntimeView.tsx`, local runtime profile UI。
- Acceptance: machine id 被呈现为 local runtime identity fact，不作为用户可编辑 authority。
- Validation: `pnpm run frontend:check`.

## M19 extension relay payload 不携带 backend_id regression test

- Scope: extension relay protocol serialization / transport adapter tests。
- Acceptance: backend target 仍属于 routing/transport 层事实，不写入 relay payload。
- Validation: targeted relay/protocol tests or serialization assertions.

