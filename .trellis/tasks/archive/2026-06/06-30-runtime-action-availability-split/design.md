# Technical Design

## Owner Split

D8 preserves three owner boundaries:

1. **AgentRun effective capability** decides whether `ext:{extension_key}` / `canvas:{mount_id}` modules are visible to the AgentRun.
2. **RuntimeGateway catalog** decides which concrete runtime actions are executable for `(RuntimeActor, RuntimeContext)`.
3. **WorkspaceModule / Extension presentation** maps visible modules, UI entries, channel methods and runtime actions into Agent/UI-facing descriptors plus readiness diagnostics.

No layer should rebuild another layer's authority. WorkspaceModule may use extension runtime projection to know extension installation identity, tab renderer loadability and protocol channel methods, but not as the executable runtime action source.

## Backend Shape

Add a narrow WorkspaceModule operation readiness model to `agentdash-contracts::surface::workspace_module`:

```rust
#[serde(rename_all = "snake_case")]
pub enum WorkspaceModuleOperationReadinessKind {
    Ready,
    MissingRuntimeGateway,
    MissingChannelTransport,
    MissingRuntimeBackendAnchor,
    BackendUnavailable,
    RuntimeActionUnavailable,
}

pub struct WorkspaceModuleOperationReadiness {
    pub kind: WorkspaceModuleOperationReadinessKind,
    pub reason: Option<String>,
}
```

`WorkspaceModuleOperation` carries `readiness: WorkspaceModuleOperationReadiness`. The field belongs to operation invocation readiness, not module visibility or renderer loadability.

Runtime action operation projection should be built from a Gateway-backed catalog:

```text
RuntimeGateway.surface_for_actor(actor, context)
  -> RuntimeActionDescriptor values
  -> join to enabled extension installation identity by action_key
  -> WorkspaceModuleOperation { dispatch: RuntimeAction, readiness }
```

Because `RuntimeActionDescriptor` does not currently carry `extension_key`, the join may use the existing Project extension projection only as an ownership index from `action_key -> extension_key`. The descriptor's schema/description/permission policy must come from RuntimeGateway catalog. Raw projection actions that are not present in the Gateway catalog are not executable operations; if kept for diagnostics, they must be marked `runtime_action_unavailable`.

`resolve_workspace_module_visibility` can remain the capability-only baseline. Runtime tools that have a delivery runtime context should use a Gateway-aware variant or builder input so list/describe see runtime action operations from the same catalog used by invoke.

## Runtime Dependency Diagnostics

Dependency diagnostics should be typed:

- Missing `RuntimeGateway` means runtime-action operations cannot be cataloged or invoked.
- Missing extension channel transport means protocol channel operations cannot be invoked.
- Missing runtime backend anchor means extension runtime/channel invocation has no target backend.
- Backend unavailable means invocation target could not be resolved at execution time.
- Extension artifact missing or action absent from Gateway catalog is `runtime_action_unavailable`.

These diagnostics do not make modules invisible. UI entries and module summaries remain visible when capability and tab loadability allow them.

## Frontend Shape

`handleExtensionWebviewBridgeRequest(runtime.invoke_action)` should stop using Project-level `extensionRuntime.projection.runtime_actions` as a preflight execution gate. The acceptable shape for this slice is:

- validate `action_key`, Project, Session and backend context;
- call `invokeAction`;
- let backend RuntimeGateway return typed denial when action is not in the actor/context catalog.

If a session runtime action surface is already available in a local frontend model, the bridge may check that surface instead. Do not add another Project-level action availability cache.

## Scope Boundaries

In scope:

- WorkspaceModule contract readiness field.
- WorkspaceModule descriptor construction and list/describe paths.
- RuntimeToolProvider dependency diagnostics if needed to feed readiness.
- Frontend bridge gate cleanup and focused tests.
- Targeted contract generation/check if generated TS changes.

Out of scope:

- Protocol channels becoming RuntimeGateway actions.
- D1 execution admission.
- D5 command policy.
- D9 VFS authorization.

## Cleanup Principle

The success condition is deleting/converging old availability paths. Do not add a fourth `available` concept. Runtime action descriptors come from Gateway, renderer `loadability` remains UI-only, and capability remains visibility-only.
