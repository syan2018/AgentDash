---
name: workspace-module-system
description: Discover, inspect, invoke, present, and compose actor-visible Workspace Module capabilities. Use when an Agent needs Canvas definitions, shared Interaction instances, installed Extensions, native workspace file/process/task Operations, one canonical call, a module UI, or bounded multi-Operation composition with OperationScript.
---

# Use Workspace Modules

Use Workspace Modules as the current Agent actor's discovery and presentation surface. Treat every
module, view, Operation, readiness value, and state projection as a server-produced snapshot. Let
the server and OperationGateway enforce current state, permission, placement, execution, and audit.

## Choose the workflow

- Discover available capabilities: call `workspace_module_list`, then `workspace_module_describe`.
- Invoke one capability: call `workspace_module_invoke`.
- Open a module UI: call `workspace_module_present`.
- Combine bounded immediate calls: call `operation_script_preflight`, then `operation_script_run`.
- Use Workflow instead when work requires durable retry, recovery, human gates, cross-session state,
  or a durable multi-step lifecycle.

## Discover current modules

1. Call `workspace_module_list` with `{}`.
2. Select a returned `module_id`.
3. Call `workspace_module_describe` with that exact `module_id`.
4. Use only Operations and views returned by the latest describe result.

Interpret module identities as follows:

- `canvas:{definition_id}`: a Canvas authoring definition. Present it to create or reuse a canonical
  Interaction attachment and open its runtime.
- `interaction:{instance_id}`: a shared Interaction runtime visible through a current attachment.
  Treat its projected state and revisions as read-only evidence, not write authority.
- `ext:{extension_key}`: an installed Extension surface.
- `builtin:vfs`: native workspace mount, read/search, and patch Operations.
- `builtin:process`: native workspace process Operations such as `shell_exec`.
- `builtin:task`: native Project Task read/write Operations.

Builtin modules have no UI view. They make explicitly exposed native tools available as canonical
Operations so they can be combined with Extension and Interaction Operations. Their invocation
still re-enters the platform tool broker and current resource authorization.

Use `canvas://{definition_id}` only as the Canvas definition preview URI and
`interaction://{instance_id}` only as the shared runtime URI returned by the server. Do not build
either URI yourself.

## Read a descriptor

Before invoking, inspect:

- `visibility`: invoke only `agent_and_panel`; treat `panel_only` as UI-only.
- `readiness`: invoke only `ready`.
- `input_schema` and `output_schema`: shape the request and interpret the result exactly.
- `effect`: distinguish reads, local mutations, and external side effects.
- `replay_policy`: decide whether a failed or uncertain call is safe to retry.
- `permission_summary` and `provenance`: use them as explanations, not as granted authority.
- `operation_ref`: copy all four fields exactly—`namespace`, `provider_key`, `operation_key`, and
  `contract_version`.

Re-run list and describe after a stale-ref, readiness, capability, attachment, or revision failure.
Do not cache, reconstruct, or guess an OperationRef.

## Invoke one Operation

Call `workspace_module_invoke` with:

```json
{
  "operation_ref": {
    "namespace": "<from describe>",
    "provider_key": "<from describe>",
    "operation_key": "<from describe>",
    "contract_version": 1
  },
  "input": {}
}
```

Match `input` to the descriptor's schema. Include any required instance ID or expected state
revision returned by the current runtime descriptor.

## Present a module

Call `workspace_module_present` with the exact `module_id`, an optional described `view_key`, and an
optional `payload`. Omit `view_key` to use `preview`.

Do not submit a renderer kind, presentation URI, title, attachment, or diagnostics authority. Use
the canonical presentation returned by the server.

## Compose immediate Operations

Use `operation_script_preflight` and `operation_script_run` only for bounded, ephemeral
composition. Copy every structured `requested_operations` entry from current describe results.

Address an allowed Operation inside Rhai with:

```text
namespace:provider_key:operation_key:v<contract_version>
```

Invoke sequentially with:

```rhai
let value = ops.invoke(
    "namespace:provider_key:operation_key:v1",
    #{ key: input.key }
);
value
```

Invoke independent calls concurrently with bounded parallelism:

```rhai
ops.invoke_all([
    #{ operation: "namespace:provider-a:first:v1", input: #{} },
    #{ operation: "namespace:provider-b:second:v1", input: #{} }
])
```

1. Send `source`, `input`, exact `requested_operations`, and optional limits to
   `operation_script_preflight`.
2. Send the identical program fields plus the unmodified returned `token` to
   `operation_script_run`.
3. Inspect the returned value, per-call evidence, `partial`, and `outcome_unknown`.
4. Re-describe and preflight again when the actor surface or descriptor changes.

Do not treat a preflight token as execution authority. Expect the server to re-admit the run and
every nested Operation. Do not assume OperationScript commits Interaction state; use an admitted
Interaction command for state changes.
