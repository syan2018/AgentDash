# Brief: Business Surface Update Paths

Active task: .trellis/tasks/06-24-release-crate-boundary-review

You are `review-business`. Review business modules that change or consume AgentRun runtime surface through session-like paths.

Write your final report to `.trellis/tasks/06-24-release-crate-boundary-review/research/04-business-surface-update-paths.md`.

Scope:

- Inspect `crates/agentdash-application/src/canvas/**`, `workspace_module/**`, `permission/**`, `capability/**`, `hooks/**`, `vfs/**`, `mcp_preset/**`, `runtime_tools/**`, `extension_runtime.rs`.
- Trace `AgentFrameBuilder`, `CapabilityState`, `RuntimeCapabilityTransition`, `adopt_persisted`, `AgentRunActiveRuntimeSurfaceAdopter`, `SessionCapabilityService`, `RuntimeSurfaceUpdateRequest`, permission grant effect classification and workspace module invoke flows.
- Separate declaration/read-only consumers from surface-changing update paths.
- Recommend the AgentRun surface update/admission facade each path should use.

Use repository evidence and file paths. Do not modify source code.
