# Backend hook runtime ownership gate notes

- Date: 2026-06-12
- Scope: session / hook runtime ownership hardening

## Model updates

- `SessionRuntime.hook_runtime_delivery_binding` now names the in-memory cache as a delivery binding. The cached runtime is not the owner; `AgentFrameHookRuntime.control_target()` is the owner.
- `SessionRuntimeRegistry::hook_runtime_delivery_binding` and `set_or_replace_hook_runtime_delivery_binding` make the delivery-cache boundary explicit.
- `SessionHookService::ensure_hook_runtime_for_target` no longer asks the delivery-session adapter to resolve the target before business use. It validates an existing binding against the requested `AgentFrameRuntimeTarget`, or rebuilds from `AgentFrame` plus `RuntimeSessionExecutionAnchor`.
- `runtime_context_transition` now writes the capability revision, re-resolves the delivery session to the current `AgentFrameRuntimeTarget`, and aligns hook runtime to that final frame. It does not call `get_hook_runtime_by_delivery_session` or `ensure_hook_runtime_for_delivery_session`.
- Runtime Canvas exposure writes the Canvas mount and workspace module grant before live capability transition, so the resulting capability revision carries both visibility facts and the hook runtime binding targets the same current frame.
- `AgentFrameBuilder` carries forward runtime visible workspace module refs across capability/context revisions, matching the runtime grant semantics used by Canvas create / present / user-open.
- `LaunchPlanner` passes `FrameLaunchEnvelope.surface.frame_id` into hook runtime resolution, so subsequent-turn planning can rebuild against the launch frame instead of letting `runtime_session_id` select a second target.

## Tests locked

- `target_first_hook_runtime_ensure_ignores_stale_delivery_resolution` proves a stale provider `resolve_runtime_hook_target(runtime_session_id)` result cannot override the requested frame target.
- `hook_business_paths_do_not_use_delivery_session_runtime_lookup` statically locks `hooks_service` and `runtime_context_transition` against reintroducing naked delivery-session hook lookup on business paths.

## Remaining adapter surface

- `SessionRuntimeInner::ensure_hook_runtime_for_delivery_session` and `get_hook_runtime_by_delivery_session` are now test-only adapter helpers used to assert delivery binding replacement. Production business paths use target-first services.
- `SessionHookService::reload_hook_runtime` and `resolve_runtime_hook_target` remain as legacy adapter/bootstrap helpers. `resolve_hook_runtime` now prefers the launch envelope frame when available.
- Tests still call `get_hook_runtime_by_delivery_session` to inspect cache contents. Those calls are assertions about delivery binding replacement, not business ownership.
