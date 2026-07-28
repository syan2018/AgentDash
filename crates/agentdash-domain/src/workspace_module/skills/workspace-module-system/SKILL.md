---
name: workspace-module-system
description: Discover, inspect, invoke, present, compose, and diagnose actor-visible Workspace Module capabilities. Use when an Agent needs Canvas definitions, shared Interaction instances, installed Extensions, native workspace file/process/task Operations, one canonical call, a module UI, bounded OperationScript composition, or an explanation for an unavailable or empty current capability surface.
---

# Use Workspace Modules

Use Workspace Modules as the current Agent actor's discovery and presentation surface. Treat every
module, view, Operation, readiness value, and state projection as a server-produced snapshot. Let
the server and OperationGateway enforce current state, permission, placement, execution, and audit.

## Choose the workflow

- Discover available capabilities: call `workspace_module_list`, then `workspace_module_describe`.
- Invoke one capability: call `workspace_module_invoke`.
- Open a module UI: call `workspace_module_present`.
- Combine bounded immediate calls: call `operation_script`.
- Use Workflow instead when work requires durable retry, recovery, human gates, cross-session state,
  or a durable multi-step lifecycle.

## Discover current modules

1. Call `workspace_module_list` with `{}`.
2. Select a returned `module_id`.
3. Call `workspace_module_describe` with that exact `module_id`.
4. Use only Operations and views returned by the latest describe result.

Read `surface_readiness` before interpreting `module_count`:

- `ready` with zero modules is an authoritative empty surface for the current execution authority.
- `degraded` includes `surface_diagnostics`; use each provider/code/message to explain which catalog
  facet is unavailable.
- A typed tool failure such as `execution_authority_*` or
  `workspace_module_platform_operation_surface_unavailable` means the current authority could not
  be resolved. It is not an empty module surface.

The platform derives builtin visibility from the same current execution authority used by
OperationGateway and the native tool broker. A changed Agent permission becomes visible after the
runtime rebind commits the new authority revision; re-run list and describe after that transition.

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

Use `operation_script` only for bounded, ephemeral composition. First describe every module you
need, then address its Operations inside Rhai with:

```text
namespace:provider_key:operation_key:v<contract_version>
```

Call `operation_script` with `source` and optional `input` only. For example, after current
descriptors confirm the two exact refs below, query VFS mounts and the current task overview
concurrently:

```json
{
  "source": "let results = ops.invoke_all([#{ operation: \"platform:vfs:mounts_list:v1\", input: #{} }, #{ operation: \"platform:task:task_read:v1\", input: #{ mode: input.task_mode } }]); #{ mounts: results[0], task_overview: results[1] }",
  "input": {
    "task_mode": "overview"
  }
}
```

Use those strings only when the latest describe result returned them. Use `ops.invoke` for
dependent or sequential calls. Use `ops.invoke_all` for independent calls; each entry requires
`operation` and `input`, and results preserve input order. The global `input` value is the tool's
JSON input. The script's final expression is its returned value.

The server fixes the Rhai dialect, host API, runtime limits, and current actor Operation manifest.
It validates and binds the program before execution, then re-admits the run and every nested
Operation against current authority. Inspect the returned value and call evidence. Re-describe and
run again after the actor surface or descriptor changes. Do not assume OperationScript commits
Interaction state; use an admitted Interaction command for state changes.

If `operation_script` returns a provider unavailable code, treat the requested catalog facet as
temporarily unresolved. Re-run list/describe after the authority or provider becomes ready; do not
replace the exact OperationRef with a guessed value.
