# RuntimeGateway dynamic action catalog 收束

## Goal

实现 design backlog Slice 2 / D7：让 `RuntimeGateway::surface_for_actor` 纳入 dynamic extension actions，使 RuntimeGateway 成为 concrete runtime action discovery 与 invoke 的同源 owner。

## Source

- Design review: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d7-runtimegateway-dynamic-action-discovery`
- Implementation slice: `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md#slice-2-runtimegateway-dynamic-action-catalog`
- Research: `.trellis/tasks/06-30-design-backlog-review/research/02-extension-action-availability.md`

## Requirements

- RuntimeGateway surface must include concrete dynamic extension action descriptors when actor/context supports them.
- Extension dynamic provider must discover Project enabled extension runtime actions for `RuntimeContext::Session`.
- Catalog and invoke must share the same concrete action resolver, so surface visibility and invocation support do not drift.
- Actor-visible `extension.runtime_action` marker descriptor must disappear from public surface.
- Static providers must continue to appear in surface unchanged.
- If context lacks Project/session facts needed for extension action discovery, surface returns no extension dynamic actions rather than marker fallback.
- No WorkspaceModule operation-source rewrite in this task; D8 handles WorkspaceModule/frontend readiness and operation projection.
- Do not keep compatibility marker action as actor-visible fallback.

## Acceptance Criteria

- [x] RuntimeGateway surface includes enabled concrete extension action descriptor(s) for a session context.
- [x] RuntimeGateway surface omits concrete extension actions when context is not session/project-scoped.
- [x] Invoke and catalog use one resolver for extension action lookup.
- [x] `extension.runtime_action` is not actor-visible in `surface_for_actor`.
- [x] Static runtime action providers still appear in surface.
- [x] Targeted runtime-gateway tests pass.

## Completion Notes

- `RuntimeGateway::surface_for_actor` is now async and merges static provider descriptors with dynamic provider discovery results.
- `ExtensionRuntimeActionProvider` resolves concrete Project enabled Session Runtime actions through one shared catalog path used by discovery, support checks, and invocation.
- `extension.runtime_action` remains only as the provider's internal marker key; actor-visible surfaces expose concrete extension action keys.
- D8 remains out of scope: WorkspaceModule operation source and frontend readiness projection are handled by the next slice.
