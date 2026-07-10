---
name: workspace-module-system
description: Use actor-visible Workspace Modules through canonical Interaction and Operation surfaces.
---

# Workspace Module System

Workspace Modules are projections, not an execution authority. Agent tools consume the
server-resolved AgentRun execution identity and the canonical OperationGateway surface.

## Tools

1. Call `workspace_module_list` to list modules visible to the current AgentRun actor.
2. Call `workspace_module_describe` with the exact `module_id` before invoking or presenting.
3. Call `workspace_module_invoke` with the complete `operation_ref` returned by describe and an
   input matching its schema. Never reconstruct a ref from an operation key. The exact ref includes
   `namespace`, `provider_key`, `operation_key`, and `contract_version`.
4. Call `workspace_module_present` with a described `module_id` and `view_key`.

## Identities and URIs

- `canvas:{definition_id}` identifies a Canvas authoring definition.
- `canvas://{definition_id}` is its definition preview URI.
- Presenting an Interaction-backed module creates or reuses a canonical instance attachment and
  returns `interaction://{instance_id}` for the shared runtime view.
- `ext:{extension_key}` identifies an installed Extension module.

Operation readiness, effect, replay policy, required capabilities, provenance, and schemas come
directly from the actor-specific canonical catalog. An unavailable operation must not be invoked.
