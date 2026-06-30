# Technical Design

## Current Split

`RuntimeGateway::surface_for_actor()` currently lists static providers only. `RuntimeGateway::invoke()` can route to dynamic providers, including extension runtime actions. The extension provider exposes a marker descriptor instead of concrete Project enabled actions.

This creates catalog/invoke drift:

- Gateway surface misses actions it can invoke.
- Project extension projection and WorkspaceModule projection become parallel discovery owners.
- Frontend and WorkspaceModule can treat manifest-level actions as executable actions without Gateway context.

## Target Shape

Add dynamic discovery to RuntimeGateway:

```rust
#[async_trait]
pub trait DynamicRuntimeActionProvider {
    async fn discover_actions(
        &self,
        actor: &RuntimeActor,
        context: &RuntimeContext,
    ) -> Result<Vec<RuntimeActionDescriptor>, RuntimeInvocationError>;
}
```

or extend the existing dynamic provider trait if that is cleaner in current code.

The Extension runtime provider should:

- Resolve concrete extension action entries from Project enabled extension runtime projection.
- Filter to `RuntimeActionKind::SessionRuntime` where appropriate.
- Emit descriptors with action key, schemas, description and policy metadata.
- Use the same resolver for `discover_actions` and `invoke`.

`extension.runtime_action` may remain as internal provider identity if needed, but it must not be returned as actor-visible runtime action surface.

## Scope Boundary

In scope:

- RuntimeGateway trait/implementation.
- Extension dynamic action provider.
- Tests for surface/invoke/catalog consistency.

Out of scope:

- WorkspaceModule operations sourced from Gateway catalog.
- Frontend bridge availability changes.
- D8 readiness diagnostics.

## Cleanup Principle

The point is to remove catalog/invoke split. Do not add a second catalog just for extension actions if RuntimeGateway surface can own the same fact directly.
