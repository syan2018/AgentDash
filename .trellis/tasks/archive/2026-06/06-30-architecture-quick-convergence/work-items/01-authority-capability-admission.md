# Work Item 01: Authority 能力准入快速修复

## Goal

修复 Authority & Capability Runtime 的两个 P0，使 PermissionGrant 的 tool-level grant 只影响执行准入，不扩大模型可见 capability/tool surface；runtime admission projection 必须按当前 `effect_frame_id` 生效，而不是按整个 run 生效。

## Source Issues

- `module-adversarial-review/adversarial-review.md` Issue 1。
- `module-adversarial-review/adversarial-review.md` Issue 2。
- `module-adversarial-review/research/08-authority-capability-runtime.md` Issue 1 / Issue 2。

## Evidence

- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:37` 注释声明 tool-level Grant 只应作为工具执行准入。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:116` 的 `apply_to_execution_capability_state` 把 admission projection 写入 `CapabilityState`。
- `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs:194` 在装配工具前替换 `context.turn.capability_state`。
- `crates/agentdash-domain/src/permission/entity.rs:17` 的 `PermissionGrant` 拥有 `run_id`、`effect_frame_id`、`source_runtime_session_id`。
- `crates/agentdash-domain/src/permission/repository.rs:38` 已有 `list_active_by_frame(effect_frame_id)`。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:318` 当前调用 `list_active_by_run(anchor.run_id)`。

## Requirements

- visible capability surface 只来自 declarative capability baseline 与 AgentFrame surface revision。
- tool-level PermissionGrant 不得向 `CapabilityState.tool.capabilities`、`enabled_clusters`、`tool_policy.include_only` 增加模型可见能力。
- runtime admission projection 必须从 runtime session anchor 定位当前/effect frame，并按 `effect_frame_id` 查询 active grants。
- run-level active grant query 只用于 audit/read model，不用于执行准入。
- 保留最小正确形态即可；完整 `AgentRunEffectiveCapabilityPort` production boundary 可留给 Design backlog。

## Suggested Implementation Shape

- 拆分 `AgentRunGrantProjection` 中“surface-changing grant”和“tool-level admission grant”的应用路径。
- 保留 frame-surface-changing grant 的现有 AgentFrame revision 更新路径。
- 对 tool-level grant：
  - 不再调用会 mutate `CapabilityState` 的路径。
  - 在 tool execution admission 或 tool call guard 处查询/应用 admission decision。
  - 如果当前 execution entry 还没有统一 admission port，先实现局部最小 guard，明确不影响 schema exposure。
- 将 `execution_capability_state_for_runtime_session` 内的 grant 查询维度改为 effect frame。

## Tests / Verification

- 后端 focused tests：
  - 同一 run 下 frame A 的 tool-level grant 不影响 frame B 的 visible tool surface。
  - tool-level grant 不增加 session tool schema exposure。
  - tool-level grant 在对应 frame 的 tool invocation admission 生效。
- 针对 `effective_capability.rs` / `tool_builder.rs` 运行相关 cargo tests。

## Out of Scope

- 不做完整 AgentRun effective/admission port 重构。
- 不处理 companion capability grant payload。
- 不设计 per-mount/path VFS authorization。

