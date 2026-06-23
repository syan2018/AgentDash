# Brief: Session Runtime Inventory

Active task: .trellis/tasks/06-24-release-crate-boundary-review

You are `review-session`. Conduct a focused architecture review of `crates/agentdash-application/src/session/**`.

Write your final report to `.trellis/tasks/06-24-release-crate-boundary-review/research/01-session-runtime-inventory.md`.

Scope:

- Count and group every file under `session`.
- Inspect `session/mod.rs`, `hub/**`, `launch/**`, `runtime_*`, `context_*`, `tool_assembly.rs`, `types.rs`, `capability`-related references and public exports.
- Identify external callers using `rg`, especially `SessionRuntimeInner`, `SessionCapabilityService`, `AgentFrameRuntimeTarget`, active runtime adoption, current backend anchor helpers and hub methods.
- Classify each group as RuntimeSession substrate, AgentRun/Lifecycle control-plane, runtime surface query/update, API adapter leakage, presentation/read-model or obsolete aggregation.
- Propose the target owner/module/crate and first child tasks.

Use repository evidence and file paths. Do not modify source code.
