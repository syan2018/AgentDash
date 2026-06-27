# Brief: AgentRun And Lifecycle Surface

Active task: .trellis/tasks/06-24-release-crate-boundary-review

You are `review-agentrun`. Conduct a focused architecture review of AgentRun/Lifecycle ownership.

Write your final report to `.trellis/tasks/06-24-release-crate-boundary-review/research/02-agentrun-lifecycle-surface.md`.

Scope:

- Inspect `crates/agentdash-application/src/agent_run/**`, `lifecycle/**`, `workflow/**`, and domain workflow entities/repositories.
- Focus on `runtime_surface.rs`, `runtime_surface_update.rs`, `effective_capability.rs`, `delivery_runtime_selection.rs`, `frame/**`, `lifecycle/dispatch_service.rs`, `session_run_context_resolver.rs`, `surface/**`.
- Determine which existing services already form the correct current surface query/update boundary and what is missing.
- Identify legitimate direct `AgentFrame` owners versus consumers that need a DTO/query facade.
- Decide how RuntimeSession should belong to AgentRun/Lifecycle while preserving clean dependency direction.

Use repository evidence and file paths. Do not modify source code.
