# Checkpoint Check Agent Prompts

All check prompts start with:

```text
Active task: .trellis/tasks/06-24-release-crate-split-draft
Branch: codex/release-crate-split-refactor
```

## Common Check Bias

- Prioritize boundary violations, stale paths, duplicate facades, incorrect chains and obsolete tests.
- Do not ask implement agents to preserve old behavior for compatibility.
- Classify each finding as `delete`, `move`, `port`, or `keep as presentation/debug read-model`.
- Assign every finding to a work item owner.
- Treat tests as evidence only when they encode target architecture.
- Recommend deleting tests that only preserve stale behavior.
- Keep output ordered by severity and wave readiness impact.

## check-boundary-ports

Check `agentdash-application-ports` after Wave 1.

Focus:

- Ports are pure DTO/trait/error.
- No `AppState`, `RepositorySet`, concrete repository/service, route DTO, `AgentFrameBuilder`, `SessionRuntimeBuilder` or concrete adapter leaks into ports.
- Existing `runtime_gateway_mcp_surface` remains reduced and does not become the full AgentRun surface DTO.
- New ports map cleanly to the target graph in `design.md`.

Minimal commands:

```powershell
cargo check -p agentdash-application-ports
rg -n "AppState|RepositorySet|AgentFrameBuilder|SessionRuntimeBuilder|agentdash_api|route" crates/agentdash-application-ports/src -g '*.rs'
```

## check-import-graph

Check static gates after import cleanup.

Focus:

- RuntimeGateway setup imports.
- AgentRun -> session implementation imports.
- Lifecycle -> AgentFrameBuilder direct links.
- AgentRun/session -> lifecycle projector/current-frame resolver links.
- API route/helper implementation DTO imports.

Commands:

```powershell
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
rg -n "crate::session::(plan|runtime_commands|types|hub|Session.*Service|LaunchCommand)" crates/agentdash-application/src/agent_run -g '*.rs'
rg -n "AgentFrameBuilder" crates/agentdash-application/src/lifecycle crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "crate::lifecycle::.*AgentRunRuntimeAddress|crate::lifecycle::surface::surface_projector|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src/agent_run crates/agentdash-application/src/session -g '*.rs'
rg -n "AgentRunRuntimeSurfaceQuery::new|AgentRunRuntimeSurfaceQueryDeps|runtime_surface_query\\(" crates/agentdash-api/src -g '*.rs'
rg -n "agentdash_application::session::(construction|plan|types|hub)|agentdash_application::agent_run::frame|agentdash_application::vfs::ResolvedVfsSurfaceSource|agentdash_application::vfs::build_surface_summary" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
```

## check-dead-paths

Check obsolete paths and tests after each major cleanup.

Focus:

- Old helper names and duplicate facades.
- Compatibility shells for obsolete module paths.
- Tests whose only purpose is preserving old route-side business assembly, launch-frame-as-current-frame semantics, direct builder construction, or Session-as-business-surface behavior.
- Deletion candidates with exact owner work item.

Search seeds:

```powershell
rg -n "session_construction|SessionCapabilityService|launch_frame_id|AgentFrameBuilder|resolve_current_frame_from_delivery_trace_ref" crates .trellis/tasks/06-24-release-crate-split-draft -g '*.rs' -g '*.md'
```

## check-wave-readiness

Check whether the next physical extraction wave can begin.

Focus:

- RuntimeGateway extraction requires MCP and setup dependencies to be port-mediated.
- RuntimeSession extraction requires launch/adoption/mailbox/effective-capability deps to be port-mediated.
- AgentRun/Lifecycle extraction requires mutual links to be ports/facades.
- VFS extraction waits for owner-specific provider dependencies to be directional.

Evidence:

- `cargo metadata --no-deps --format-version 1`
- Static gates in `implement.md`
- Worker handoffs from the current wave
