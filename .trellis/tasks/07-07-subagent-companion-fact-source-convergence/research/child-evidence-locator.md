# Research: child evidence locator

- Query: child evidence locator / parent-visible child AgentRun journal and lifecycle evidence.
- Scope: internal
- Date: 2026-07-07

## Findings

### 1. Summary recommendation

Parent-visible child evidence should be exposed as structured `result_refs.evidence[]` locators, not as child-local `lifecycle://session/...` URIs.

Recommended contract:

```json
{
  "schema_version": 1,
  "result_refs": {
    "gate_id": "...",
    "child": {
      "run_id": "...",
      "agent_id": "...",
      "frame_id": "...",
      "delivery_runtime_session_id": "..."
    },
    "evidence": [
      {
        "kind": "agent_run_journal",
        "scope": "child_agent_run",
        "child_run_id": "...",
        "child_agent_id": "...",
        "child_frame_id": "...",
        "delivery_runtime_session_id": "...",
        "cursor": null
      },
      {
        "kind": "lifecycle_file",
        "scope": "child_delivery_session",
        "child_run_id": "...",
        "child_agent_id": "...",
        "child_frame_id": "...",
        "delivery_runtime_session_id": "...",
        "mount_id": "lifecycle",
        "path": "session/events.json"
      },
      {
        "kind": "lifecycle_file",
        "scope": "child_delivery_session",
        "child_run_id": "...",
        "child_agent_id": "...",
        "child_frame_id": "...",
        "delivery_runtime_session_id": "...",
        "mount_id": "lifecycle",
        "path": "session/messages"
      },
      {
        "kind": "runtime_trace",
        "scope": "child_delivery_session",
        "child_run_id": "...",
        "child_agent_id": "...",
        "child_frame_id": "...",
        "delivery_runtime_session_id": "..."
      }
    ]
  }
}
```

The locator should carry coordinates and relative evidence intent. The resolver should derive any product URL, VFS `surface_ref`, route, or stream query at read time.

Resolver ownership:

- `AgentRunJournalService` owns journal evidence because it maps raw RuntimeSession events into a parent-visible `agentrun:{run}:{agent}` journal sequence.
- `AgentRunResourceSurfaceQuery` + `VfsSurfaceResolver` own lifecycle file evidence because they construct the parent-visible VFS surface and lifecycle mount from AgentRun/runtime refs.
- `runtime_traces` route remains a diagnostic fallback for exact runtime trace inspection by `delivery_runtime_session_id`.
- A thin `AgentRunEvidenceLocatorResolver` should be added in the AgentRun application/API boundary only if callers need one stable "open evidence" DTO. It should validate child coordinates and delegate to the existing owners above; it should not live in companion, wait, mailbox, or LifecycleGate.

Important exactness rule:

- `ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id }` resolves the current delivery for that AgentRun, so it is product-friendly but can drift after rerun.
- `ResolvedVfsSurfaceSource::SessionRuntime { session_id }` resolves an exact delivery RuntimeSession through `RuntimeSessionExecutionAnchor`, so it is appropriate when reading the evidence for the specific child result attempt.
- The locator should include both child run/agent/frame and `delivery_runtime_session_id`; the resolver should either use the exact runtime session surface or assert that the AgentRun current delivery still matches the evidence runtime session before using the AgentRun surface.

### 2. Existing surfaces/routes/services with file:line anchors

Files found:

- `crates/agentdash-application-ports/src/vfs_surface_runtime.rs` - defines external VFS surface refs, including `session-runtime:{session}` and `agent-run:{run}:{agent}`.
- `crates/agentdash-application/src/vfs_surface_resolver.rs` - resolves a surface source into a concrete `Vfs`.
- `crates/agentdash-api/src/routes/vfs_surfaces.rs` - exposes `/vfs-surfaces/*` routes for resolve, list, and read.
- `crates/agentdash-application-ports/src/lifecycle_surface_projection.rs` - defines `AgentRunLifecycleSurfaceInput` and lifecycle surface modes.
- `crates/agentdash-application-lifecycle/src/lifecycle/vfs_mount.rs` - builds the `lifecycle` mount with run/agent/session metadata.
- `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs` - reads/lists `agent_run_session` lifecycle evidence paths.
- `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs` - resolves AgentRun or RuntimeSession resource surfaces.
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs` - maps RuntimeSession event streams into AgentRun journal streams/pages.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - exposes AgentRun journal routes and AgentRun list child projection.
- `crates/agentdash-api/src/routes/runtime_traces.rs` - checks runtime trace permission by `RuntimeSessionExecutionAnchor`.
- `crates/agentdash-domain/src/workflow/agent_lineage.rs` - same-run child/subagent lineage model.
- `crates/agentdash-domain/src/workflow/agent_run_lineage.rs` - cross-run fork lineage model.
- `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs` - current delivery binding and terminal evidence fields.
- `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs` - current wait result refs surface.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs` - terminal fallback gate result payload and mailbox wake intent.
- `crates/agentdash-application-workflow/src/gate/resolver.rs` - normal companion result payload shape.
- `crates/agentdash-application/src/companion/tools.rs` - companion mailbox source/dedup and delivery into mailbox.
- `crates/agentdash-application/src/companion/gate_control.rs` - current free-text parent result mailbox rendering.

Code patterns:

- VFS external surface refs already distinguish exact runtime session from AgentRun product surface: `SessionRuntime { session_id }` and `AgentRun { run_id, agent_id }` are variants in `ResolvedVfsSurfaceSource` (`crates/agentdash-application-ports/src/vfs_surface_runtime.rs:28`, `crates/agentdash-application-ports/src/vfs_surface_runtime.rs:31`), rendered as `session-runtime:{session}` and `agent-run:{run}:{agent}` (`crates/agentdash-application-ports/src/vfs_surface_runtime.rs:60`, `crates/agentdash-application-ports/src/vfs_surface_runtime.rs:63`).
- `VfsSurfaceResolver` resolves `SessionRuntime` through `resource_surface_for_runtime_session` (`crates/agentdash-application/src/vfs_surface_resolver.rs:186`) and `AgentRun` through `resource_surface_for_agent_run` (`crates/agentdash-application/src/vfs_surface_resolver.rs:197`).
- API surface permission uses runtime-trace permission only for `SessionRuntime`; all other surfaces, including `AgentRun`, use project permission (`crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:53`, `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:59`).
- VFS routes expose stable parent-visible operations: resolve (`crates/agentdash-api/src/routes/vfs_surfaces.rs:39`), get by `surface_ref` (`crates/agentdash-api/src/routes/vfs_surfaces.rs:43`), list mount entries (`crates/agentdash-api/src/routes/vfs_surfaces.rs:47`), and read file (`crates/agentdash-api/src/routes/vfs_surfaces.rs:51`). `read_surface_file` parses `surface_ref`, resolves the VFS, checks mount availability, and calls `vfs_service.read_text` with `mount_id + path` (`crates/agentdash-api/src/routes/vfs_surfaces.rs:191`, `crates/agentdash-api/src/routes/vfs_surfaces.rs:196`, `crates/agentdash-api/src/routes/vfs_surfaces.rs:199`, `crates/agentdash-api/src/routes/vfs_surfaces.rs:205`).
- Lifecycle surface projection is typed by `AgentRunLifecycleSurfaceInput`, which carries `address`, optional `message_stream`, `project_id`, mode, and node evidence (`crates/agentdash-application-ports/src/lifecycle_surface_projection.rs:488`, `crates/agentdash-application-ports/src/lifecycle_surface_projection.rs:491`, `crates/agentdash-application-ports/src/lifecycle_surface_projection.rs:493`, `crates/agentdash-application-ports/src/lifecycle_surface_projection.rs:496`).
- The lifecycle mount root includes run/agent/session in metadata and `root_ref`, but provider dispatch relies on mount metadata and mount-relative path, not a raw child-local URI (`crates/agentdash-application-lifecycle/src/lifecycle/vfs_mount.rs:16`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_mount.rs:21`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_mount.rs:35`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_mount.rs:38`).
- `lifecycle_vfs` `agent_run_session` read paths include `state`, `execution-log`, `session/*`, `node/*`, and `orchestration/state` (`crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:89`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:96`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:102`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:107`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:112`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:146`).
- `lifecycle_vfs` list paths expose `session/messages`, `session/tools`, `session/tool-results`, `session/writes`, `session/summaries`, `session/terminal`, and `session/turns` (`crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:190`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:199`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:208`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:241`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:250`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:254`, `crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:263`).
- `lifecycle_vfs` derives session event source from mount metadata as `AgentRunJournalRef(run_id, agent_id)` (`crates/agentdash-application-lifecycle/src/lifecycle/vfs_provider.rs:469`), so the same relative `session/messages` path means "messages for whichever run/agent/session the mount was built for".
- `AgentRunResourceSurfaceQuery::resource_surface_for_runtime_session` resolves a runtime session, then projects a lifecycle surface (`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:133`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:145`). `resource_surface_for_agent_run` resolves the current runtime surface for run/agent and then projects the lifecycle surface (`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:148`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:153`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:169`).
- The current AgentRun resource surface projection installs `WorkspaceReadSurface` with message stream `runtime_session_id` (`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:191`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:196`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:202`).
- Runtime-session surface resolution is exact: it loads `RuntimeSessionExecutionAnchor` by session id, then verifies run/agent/project/frame consistency (`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:233`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:238`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:282`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:304`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:316`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:384`).
- AgentRun surface resolution by run/agent selects current delivery before resolving the surface (`crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:489`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:495`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:503`, `crates/agentdash-application-agentrun/src/agent_run/runtime_surface.rs:510`).
- AgentRun journal service exposes `AgentRunJournalQuery { run_id, agent_id, delivery_runtime_session_id }` (`crates/agentdash-application-agentrun/src/agent_run/journal.rs:113`) and maps inherited lineage plus current delivery into monotonic `journal_seq` events with source runtime metadata (`crates/agentdash-application-agentrun/src/agent_run/journal.rs:35`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:37`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:40`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:41`).
- `subscribe_visible_journal_stream` requires a delivery runtime session, loads inherited prefix, subscribes to the current delivery runtime stream, and builds an AgentRun journal stream (`crates/agentdash-application-agentrun/src/agent_run/journal.rs:196`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:201`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:212`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:219`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:224`).
- AgentRun journal product identity is `agentrun:{run_id}:{agent_id}` and events are projected by rewriting `PersistedSessionEvent.session_id`, `event_seq`, and `notification.session_id` (`crates/agentdash-application-agentrun/src/agent_run/journal.rs:503`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:507`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:512`, `crates/agentdash-application-agentrun/src/agent_run/journal.rs:514`).
- AgentRun journal routes already exist for page and stream: `/agent-runs/{run_id}/agents/{agent_id}/journal/events` and `/agent-runs/{run_id}/agents/{agent_id}/journal/stream/ndjson` (`crates/agentdash-api/src/routes/lifecycle_agents.rs:160`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:177`). The page route constructs `AgentRunJournalQuery` from resolved AgentRun context (`crates/agentdash-api/src/routes/lifecycle_agents.rs:1017`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1023`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1033`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1035`). The stream route calls `subscribe_visible_journal_stream` with the same query (`crates/agentdash-api/src/routes/lifecycle_agents.rs:1200`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1217`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:1219`).
- The frontend stream transport consumes the AgentRun journal stream by run/agent refs (`packages/app-web/src/features/session/model/streamTransport.ts:30`).
- Runtime trace route exists at `/runtime-traces/{id}` (`crates/agentdash-api/src/routes/lifecycle_views.rs:53`) and checks permission by finding the RuntimeSessionExecutionAnchor, loading its LifecycleRun, and checking Project permission (`crates/agentdash-api/src/routes/runtime_traces.rs:18`, `crates/agentdash-api/src/routes/runtime_traces.rs:31`, `crates/agentdash-api/src/routes/runtime_traces.rs:44`, `crates/agentdash-api/src/routes/runtime_traces.rs:45`).
- Same-run child/subagent relation is `AgentLineage`, explicitly separate from runtime session lineage (`crates/agentdash-domain/src/workflow/agent_lineage.rs:5`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:7`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:9`). It carries `run_id`, optional `parent_agent_id`, `child_agent_id`, relation kind, and source frame (`crates/agentdash-domain/src/workflow/agent_lineage.rs:11`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:13`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:14`, `crates/agentdash-domain/src/workflow/agent_lineage.rs:17`).
- Cross-run fork relation is `AgentRunLineage`, carrying parent and child run/agent ids plus frame baselines (`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:12`, `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:14`, `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:16`, `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:20`, `crates/agentdash-domain/src/workflow/agent_run_lineage.rs:24`).
- Delivery binding holds exact `runtime_session_id`, `launch_frame_id`, terminal state/message, and status (`crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:67`, `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:70`, `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:71`, `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:78`, `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:84`, `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:86`).
- Wait currently returns only gate-level refs (`gate_id`, `run_id`, `agent_id`, `frame_id`, `gate_kind`) from `gate_item_from_gate` (`crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:10`, `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:19`).
- Gate terminal fallback currently writes a generic summary plus terminal state/message, delivery trace ref, resolved turn id, failure kind, and source into the gate payload (`crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:211`, `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:220`, `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:224`, `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:225`, `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:227`, `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:229`).
- Normal `complete_child_result` gate payload currently includes child agent id and delivery trace, but not a parent-visible evidence locator array (`crates/agentdash-application-workflow/src/gate/resolver.rs:377`, `crates/agentdash-application-workflow/src/gate/resolver.rs:387`, `crates/agentdash-application-workflow/src/gate/resolver.rs:391`, `crates/agentdash-application-workflow/src/gate/resolver.rs:410`, `crates/agentdash-application-workflow/src/gate/resolver.rs:411`).
- Companion mailbox wake has structured source identity and dedup key (`crates/agentdash-application/src/companion/tools.rs:224`, `crates/agentdash-application/src/companion/tools.rs:232`, `crates/agentdash-application/src/companion/tools.rs:317`, `crates/agentdash-application/src/companion/tools.rs:333`) but still injects text via `text_user_input_blocks(input.input_text)` (`crates/agentdash-application/src/companion/tools.rs:519`, `crates/agentdash-application/src/companion/tools.rs:523`, `crates/agentdash-application/src/companion/tools.rs:527`).
- Existing parent result mailbox text starts with `Companion child result is available.` (`crates/agentdash-application/src/companion/gate_control.rs:1136`, `crates/agentdash-application/src/companion/gate_control.rs:1145`) and should not become the evidence locator authority.
- AgentRun list already inlines child nodes with real shell status via `AgentRunListChild` (`crates/agentdash-contracts/src/runtime/workflow.rs:1432`, `crates/agentdash-contracts/src/runtime/workflow.rs:1444`, `crates/agentdash-contracts/src/runtime/workflow.rs:1450`). The route builds inline children from AgentLineage and list-item resolution (`crates/agentdash-api/src/routes/lifecycle_agents.rs:246`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:258`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:261`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:337`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:367`, `crates/agentdash-api/src/routes/lifecycle_agents.rs:371`).

### 3. Why child-local lifecycle URI is wrong in parent view

`lifecycle://session/...` is a connector/session-scoped readable address inside a lifecycle mount. It is not a product-level, parent-visible evidence locator by itself.

Reasons:

- The VFS access model is `surface_ref + mount_id + mount_relative_path`, not a globally readable URI. A parent view must first resolve the correct surface and mount, then read a mount-relative path.
- The `lifecycle` mount is unique per resolved surface and is built from that surface's run/agent/session metadata. If the parent surface has a `lifecycle` mount, `session/messages` means the parent delivery session's messages. The same relative path on the child surface means the child delivery session's messages.
- `lifecycle_vfs` derives the journal source from mount metadata (`run_id + agent_id`), not from a raw child URI. A child-local `lifecycle://session/events.json` string lacks the product coordinates needed to select the child mount.
- AgentRun journal rewrites raw RuntimeSession events into `agentrun:{run}:{agent}` coordinates and monotonic journal sequence. A raw RuntimeSession event seq or URI is not the parent-visible AgentRun journal cursor.
- `AgentRun` surface refs select current delivery. A child result needs evidence for the delivery that produced the result, so the locator must carry `delivery_runtime_session_id` and the resolver must avoid silently reading a later child delivery.
- Permission checks live at the route/resolver boundary. A raw child-local lifecycle URI bypasses the existing `ResolvedVfsSurfaceSource` / runtime-trace anchor permission model.

### 4. Proposed locator contract and resolver owner

`result_refs.evidence[]` should be a bounded array of typed locator objects. It should not embed large evidence bodies. It should not embed direct `lifecycle://session/...` strings.

Suggested fields:

| Field | Required | Meaning |
| --- | --- | --- |
| `kind` | yes | `agent_run_journal`, `lifecycle_file`, `runtime_trace`, later `mailbox_message` if needed. |
| `scope` | yes | `child_agent_run` or `child_delivery_session`. |
| `child_run_id` | yes | Product run coordinate. For same-run companion child, this is the parent run id. For forked AgentRun, this is the fork child run id. |
| `child_agent_id` | yes | Child/subagent agent coordinate. |
| `child_frame_id` | recommended | Child frame coordinate at dispatch/result time. |
| `delivery_runtime_session_id` | yes for delivery evidence | Exact RuntimeSession that produced the result. |
| `mount_id` | only lifecycle file | Usually `lifecycle`. |
| `path` | only lifecycle file | Mount-relative path such as `session/events.json`, `session/messages`, `session/tools`, `session/turns`, `session/terminal`. |
| `journal_after_seq` / `cursor` | optional | Product journal cursor when a bounded page is desired. |
| `label` | optional | UI label only; not resolver input. |

Recommended initial evidence entries:

- `agent_run_journal` for the child AgentRun page/stream.
- `lifecycle_file` with `path = "session/events.json"` for raw structured event projection.
- `lifecycle_file` with `path = "session/messages"` for message index.
- `lifecycle_file` with `path = "session/tools"` for tool index.
- `lifecycle_file` with `path = "session/turns"` for turn-scoped evidence.
- `lifecycle_file` with `path = "session/terminal"` when terminal output matters.
- `runtime_trace` for the diagnostic route keyed by `delivery_runtime_session_id`.

Resolver behavior:

1. Validate locator schema and recognized `kind`.
2. Validate child coordinates:
   - same-run companion/subagent: `AgentLineage` should prove `parent_run_id == child_run_id` and parent/child agent relation when parent context is known.
   - cross-run fork: `AgentRunLineage` should prove parent/child run relation when cross-run evidence is in scope.
3. Validate `delivery_runtime_session_id`:
   - load `RuntimeSessionExecutionAnchor` and assert `anchor.run_id == child_run_id`, `anchor.agent_id == child_agent_id`, and `anchor.launch_frame_id` matches `child_frame_id` when present.
4. Resolve by kind:
   - `agent_run_journal`: call `AgentRunJournalService` with `AgentRunJournalQuery { run_id: child_run_id, agent_id: child_agent_id, delivery_runtime_session_id }`. Existing public routes currently derive delivery session from current AgentRun context, so a dedicated evidence resolver route may need to pass the exact session id internally.
   - `lifecycle_file`: prefer `ResolvedVfsSurfaceSource::SessionRuntime { session_id: delivery_runtime_session_id }` for exact delivery reads, then read `mount_id + path` through the existing VFS surface service. If using `AgentRun { child_run_id, child_agent_id }`, assert current delivery matches `delivery_runtime_session_id`.
   - `runtime_trace`: use `/runtime-traces/{delivery_runtime_session_id}` or the underlying runtime trace service after permission/anchor checks.

Owner recommendation:

- Add the thin resolver in `agentdash-application-agentrun` or API route layer, next to `AgentRunResourceSurfaceQuery` / `AgentRunJournalService`, because that layer already understands AgentRun control-plane identity, delivery runtime selection, and journal projection.
- Keep `LifecycleGate` and wait result payloads as locator producers only. They should not resolve VFS paths.
- Keep `lifecycle_vfs` as the file provider only. It should not know parent-child business relation; it only serves a resolved mount.
- Keep mailbox as delivery envelope only. It may carry locator refs in `payload_json`, but it should not own evidence resolution.

### 5. Tests/contract checks needed

Backend contract tests:

- Gate terminal fallback result payload includes `result_refs.child` and `result_refs.evidence[]` with `child_run_id`, `child_agent_id`, `child_frame_id`, and `delivery_runtime_session_id`.
- Normal `companion_respond` / `complete_child_result` result payload includes the same evidence locator contract.
- No `result_refs.evidence[]` entry contains a raw `lifecycle://session/...` URI.
- Wait `gate_item_from_gate` returns the gate payload's evidence refs, or enough result refs to reach them, so `wait(activity_refs=[gate_id])` exposes child evidence consistently.
- Evidence resolver rejects a locator whose `delivery_runtime_session_id` anchor points to a different run/agent/frame.
- Evidence resolver rejects a same-run child locator when `AgentLineage` does not prove the parent/child relation, when parent context is supplied.
- Evidence resolver rejects a cross-run child locator when `AgentRunLineage` does not prove the parent/child relation, when fork evidence is supplied.
- Evidence resolver reads `lifecycle_file` via exact `SessionRuntime` surface and returns `session/events.json` or `session/messages` from the child delivery session, not the parent lifecycle mount.
- AgentRun journal evidence returns `agentrun:{child_run}:{child_agent}` session id and journal sequence while preserving source runtime session metadata.
- AgentRun surface fallback path detects current delivery drift: if `agent-run:{child_run}:{child_agent}` current delivery no longer equals locator `delivery_runtime_session_id`, resolver must use exact `SessionRuntime` or return a clear conflict.
- Runtime trace diagnostic route confirms permission through `RuntimeSessionExecutionAnchor` and project `Use` permission.

Integration/static checks:

- Search result refs for `lifecycle://session/` and fail if emitted as parent-visible child evidence.
- Search remaining `Companion child result is available` use and ensure it is only bounded projection text, not evidence authority.
- Contract generation includes any new evidence locator DTO if the resolver route or generated contracts expose it to frontend.
- Frontend / API tests cover opening child journal evidence from a parent result ref without requiring the parent workspace stream to be open.
- Frontend / API tests cover VFS read of `surface_ref=session-runtime:{child_delivery_runtime_session_id}`, `mount_id=lifecycle`, `path=session/events.json`.

### Related specs

- `.trellis/spec/backend/vfs/vfs-access.md`: VFS address model, AgentRun lifecycle session mount, lifecycle provider paths.
- `.trellis/spec/backend/session/streaming-protocol.md`: AgentRun journal stream contract and runtime trace diagnostic read surface.
- `.trellis/spec/cross-layer/backbone-protocol.md`: Backbone persisted session event and AgentRun journal stream semantics.
- `.trellis/spec/backend/session/agentrun-mailbox.md`: mailbox as delivery envelope, wait as watcher, result refs as bounded refs.
- `.trellis/spec/backend/workflow/activity-lifecycle.md`: LifecycleGate wait policy terminal convergence and producer terminal fallback.
- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/prd.md`: requirement 13 forbids child-local lifecycle URI as parent-visible result ref.
- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/design.md`: child evidence locator target shape and authority model.
- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/implement.md`: Slice A evidence research requirement.

### External references

None. This slice is internal architecture research over existing repo contracts and task artifacts.

## Caveats / Not Found

- I did not find an existing dedicated `AgentRunEvidenceLocatorResolver` or typed evidence locator DTO. Existing pieces are present, but the stable "one locator in result_refs -> open/read evidence" contract still needs a thin resolver or route-layer adapter.
- Existing public AgentRun journal routes derive `delivery_runtime_session_id` from current AgentRun context. The underlying `AgentRunJournalService` can accept an explicit delivery runtime session id, but the public route shape does not currently expose exact historical delivery selection.
- `ResolvedVfsSurfaceSource::AgentRun` is product-friendly but current-delivery based. Evidence for a completed child result should not rely on it unless current delivery is checked against the locator runtime session.
- Existing wait `result_refs` are gate-level only; they do not yet surface child evidence refs.
- Existing terminal fallback payload has `delivery_trace_ref` and terminal message, but not a structured child evidence array.
- Existing companion result wake still renders a free-text parent notice and injects it as text user input blocks; that path should carry locator refs only as projection metadata, not become the evidence authority.
