---
name: workspace-module-system
description: Use actor-visible Workspace Modules through canonical Interaction and Operation surfaces.
---

# Workspace Module System

Workspace Module 是当前 Agent actor 可见的 module、UI entry 与 canonical Operation descriptor
投影。它不持有 execution、state、permission 或 placement authority：

- OperationGateway 是 Operation discovery、admission、dispatch、result 与 audit 的唯一权威。
- Interaction service 是 definition、instance、attachment 与 canonical state 的唯一权威。
- Extension runtime action、protocol method 与 backend service 都只通过 canonical Operation 暴露。
- AgentRun 每次调用工具时都会由服务端解析 immutable AgentFrame、applied resource surface 和
  current Operation surface。

## Tools

1. Call `workspace_module_list` to list modules visible to the current AgentRun actor.
2. Call `workspace_module_describe` with the exact `module_id` before invoking or presenting.
3. Call `workspace_module_invoke` with the complete `operation_ref` returned by describe and an
   input matching its schema. Never reconstruct a ref from an operation key. The exact ref includes
   `namespace`, `provider_key`, `operation_key`, and `contract_version`.
4. Call `workspace_module_present(module_id, view_key?, payload?)`. The server derives renderer,
   presentation URI, title, diagnostics and any Interaction attachment. Never submit or synthesize
   those fields.
5. If the surface changes, call list/describe again. Do not cache or reconstruct refs,
   presentation URIs or readiness.

## Identities and URIs

- `canvas:{definition_id}` identifies a Canvas authoring definition.
- `canvas://{definition_id}` is its definition preview URI.
- Presenting a Canvas creates or reuses a canonical instance attachment and returns
  `interaction://{instance_id}`.
- `interaction:{instance_id}` identifies a shared runtime module visible only through a current
  AgentRun attachment. Describe returns its pinned definition revision, state revision, typed
  command Operations and the V1 allowlisted Agent state projection. Projection values are
  read-only evidence, not write authority.
- `interaction://{instance_id}` opens that shared runtime view.
- `ext:{extension_key}` identifies an installed Extension module.

## Descriptor fields

- `visibility`: `agent_and_panel` is Agent-callable; `panel_only` is UI-only.
- `readiness`: only `ready` Operations can be invoked.
- `input_schema` / `output_schema`: validate the exact request and result shape.
- `effect`: distinguishes read, local mutation and external side effect.
- `replay_policy`: distinguishes non-replayable, idempotent and replay-safe behavior.
- `permission_summary` and `provenance`: explain current admission requirements and canonical
  source. They are descriptive; the server still re-admits every call.

## Single invoke, immediate composition, durable orchestration

Use `workspace_module_invoke` for one exact Operation.

Use `operation_script_preflight` followed by `operation_script_run` for one bounded immediate
composition:

1. Select at least two complete OperationRefs from current describe results when composition is
   needed.
2. Submit `source`, `input`, `requested_operations`, optional `language` (`rhai_v1`),
   `host_api_version` (`1`) and optional limits to preflight.
3. Submit the identical program plus the unmodified preflight `token` to run.
4. Inspect returned value, per-call evidence, `partial` and `outcome_unknown`.

The server rebuilds descriptor digests, effect/replay metadata, principal, scope, authority revision
and granted capabilities from the current surface. A preflight token does not bypass run or nested
Operation admission.

Use Workflow when work needs durable retry, recovery, human gates, cross-session state or a durable
multi-step lifecycle. OperationScript is ephemeral and does not commit Interaction state by itself.

## Component events

Canvas component events use a definition-owned tagged target: versioned platform command, exact
single Operation, or ephemeral OperationScript. Event payload is schema-validated and passed
through as input. Inline Rhai or a `.rhai` file in the pinned immutable SourceBundle executes in the
trusted UserWorkshop host; the iframe never executes Rhai and never receives credentials, placement
or bearer authority.
