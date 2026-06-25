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

## Round 3 Check Prompts

All Round 3 check agents start with:

```text
Active task: .trellis/tasks/06-24-release-crate-split-draft
Branch: codex/release-crate-split-refactor
Round: 3 checkpoint check
Dispatch: .trellis/tasks/06-24-release-crate-split-draft/dispatch-round-3.md
Checkpoint baseline: .trellis/tasks/06-24-release-crate-split-draft/checkpoint-wave-2.md
```

Round 3 check agents verify worker output only after implement workers finish. They do not ask for old behavior to be preserved for compatibility, and they do not run large workspace tests before narrow gates pass.

### check-session-adoption-port

Focus:

- Session/AgentRun live adoption direction after `session-adoption-port-impl`.
- Production wiring should consume `RuntimeSurfaceAdoptionPort` where the dependency crosses Session/AgentRun.
- Remaining `AgentRunActiveRuntimeSurfaceAdopter` paths must be classified as `delete`, `move`, `port`, or `keep` with reason.
- RuntimeSession extraction readiness must be stated explicitly.

Commands:

```powershell
cargo check -p agentdash-application
rg -n "AgentRunActiveRuntimeSurfaceAdopter|ActiveRuntimeSurfaceAdopter" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "RuntimeSurfaceAdoptionPort" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap crates/agentdash-application/src/agent_run -g '*.rs'
```

### check-session-launch-commit-port

Focus:

- Session launch dependency direction after `session-launch-commit-port-impl`.
- Session launch should not import AgentRun implementation adapters for launch envelope or accepted launch commit.
- Tests that only anchor the old adapter chain should be recommended for deletion.

Commands:

```powershell
cargo check -p agentdash-application
rg -n "FrameLaunchEnvelopeProvider|SharedFrameLaunchEnvelopeProvider|AgentRunAcceptedLaunchCommitAdapter|AgentRunAcceptedLaunchCommitInput" crates/agentdash-application/src/session crates/agentdash-api/src/bootstrap -g '*.rs'
rg -n "frame_launch_envelope|accepted_launch|launch_commit" crates/agentdash-application-ports/src crates/agentdash-application/src/session -g '*.rs'
```

### check-control-dispatch-boundary

Focus:

- AgentRun must not construct `LifecycleDispatchService` directly.
- AgentRun frame construction must not import Lifecycle helper implementation paths.
- Remaining Lifecycle dispatch usage inside Lifecycle/workflow owner paths should be classified separately from AgentRun violations.

Commands:

```powershell
cargo check -p agentdash-application
rg -n "LifecycleDispatchService" crates/agentdash-application/src/agent_run crates/agentdash-application/src/workflow/orchestration -g '*.rs'
rg -n "composer_lifecycle_node|resolve_current_frame_from_delivery_trace_ref|crate::lifecycle" crates/agentdash-application/src/agent_run/frame/construction -g '*.rs'
```

### check-vfs-owner-adapters

Focus:

- Generic VFS ownership after `vfs-owner-adapter-prep-impl`.
- Session/lifecycle/canvas provider wiring should live in owner adapters or remain classified as blockers.
- Physical VFS extraction readiness must be stated explicitly.

Commands:

```powershell
cargo check -p agentdash-application
rg -n "crate::session|crate::lifecycle|crate::canvas|provider_lifecycle|mount_canvas" crates/agentdash-application/src/vfs -g '*.rs'
rg -n "ResolvedVfsSurfaceSource|build_surface_summary" crates/agentdash-api/src crates/agentdash-application/src/vfs -g '*.rs'
```

### check-gateway-regression

Focus:

- Gateway extracted crate must not regain a dependency on monolithic `agentdash-application`.
- API/local/MCP must not reintroduce `agentdash_application::runtime_gateway` imports.
- Temporary `agentdash-application` umbrella re-export is allowed only until visibility cleanup.

Commands:

```powershell
cargo check -p agentdash-application-runtime-gateway
cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp
rg -n "agentdash_application::runtime_gateway" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
rg -n "agentdash_application::|crate::(mcp_preset|workspace|agent_run|lifecycle|session|vfs|canvas)::" crates/agentdash-application-runtime-gateway/src -g '*.rs'
```
