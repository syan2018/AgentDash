# Source Documents

This directory stores source text copied from the user's pasted attachments for the Dynamic Workflows research task.

## Files

- `claude-dynamic-workflows-official-doc-zh-cn.md`
  - Source: Claude Code Dynamic Workflows official docs, Simplified Chinese page text pasted into Codex.
  - Original attachment: `C:\Users\Syan\.codex\attachments\eb234242-cfb0-41b0-a46b-98ed35c00340\pasted-text.txt`

- `claude-dynamic-workflows-article-zhihu-simpread.md`
  - Source: SimpRead-converted Chinese article discussing Claude Code Dynamic Workflows.
  - Original attachment: `C:\Users\Syan\.codex\attachments\79de185a-0bc7-414b-8d05-87a4e2392039\pasted-text.txt`

- `current-code-context.md`
  - Source: AgentDash local source review on 2026-06-06.
  - Purpose: records current Lifecycle / WorkflowGraph / ProjectAgent / MCP / persistence / local execution facts with source locations for later design review.

- `claude-workflow-behavior-coverage.md`
  - Source: behavior matrix distilled from the two Claude Dynamic Workflows references plus AgentDash code/spec review.
  - Purpose: tracks core workflow semantics AgentDash should cover as an architecture pressure test, without requiring one-to-one Claude Code product replication.

- `follow-up-module-roadmap.md`
  - Source: integration summary after the first session-scoped API migration and three module research passes.
  - Purpose: gives the recommended implementation order for `orchestration-domain-contract`, `workflow-graph-compiler`, `common-orchestration-runtime-static-graph`, trace anchor convergence, and dynamic script compiler.

- `orchestration-domain-contract-plan.md`
  - Source: AgentDash source/spec review focused on `LifecycleRun`, `WorkflowGraphInstance`, Activity runtime state, repository mapping, and migrations.
  - Purpose: defines the first implementation slice for `LifecycleContext`, `OrchestrationInstance`, `OrchestrationPlanSnapshot`, `RuntimeNodeState`, and `StateExchangeSnapshot`.

- `workflow-graph-compiler-plan.md`
  - Source: AgentDash graph/activity/compiler surface review.
  - Purpose: maps existing `WorkflowGraph` semantics into a deterministic `OrchestrationPlanSnapshot` compiler with fixtures and diagnostics.

- `common-runtime-convergence-plan.md`
  - Source: AgentDash engine/scheduler/executor/orchestrator/persistence review.
  - Purpose: plans the migration from `WorkflowGraphInstance.activity_state` to common orchestration runtime snapshot/journal and graph-compatible projection.

These files are copied verbatim for local review continuity. If external behavior needs current verification, prefer official Claude Code documentation.
