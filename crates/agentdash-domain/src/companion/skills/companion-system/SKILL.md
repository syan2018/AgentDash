---
name: companion-system
description: AgentDash companion interaction protocol. Use when an AgentDashboard session needs companion_request/companion_respond to ask a human, request platform capability grants, coordinate parent/sub-session work, return structured review or completion results, or route model-authored dynamic workflow proposals such as research fanout, review pipelines, runbooks, and local-effect workflows through human/platform review before execution.
---

# Companion System

Use this skill when a session has companion tools and needs structured cross-subject interaction.

## Core Flow

1. Choose the companion target: `human`, `platform`, `parent`, or `sub`.
2. Send `companion_request` with a JSON object `payload`.
3. Use a registered `payload.type` when the intent matches a known protocol.
4. Wait only when the next action depends on the response.
5. Adopt `companion_respond` through the structured response fields, not through free-form text alone.

## Core Rules

- `companion_request` is the active interaction entrypoint. It carries intent and routing metadata.
- `companion_respond` returns a structured response to a known `request_id`.
- `payload` is always a JSON object. Use `payload.type` for registered protocols.
- Request message bodies use `payload.message` for `task`, `review`, `approval`, and `notification`.
- Companion events record interaction continuity. Permission, grant, and runtime capability state remain the authority for tool access.

## Target Selection

- `human`: ask the current user for approval, choice, missing information, or scope confirmation.
- `platform`: ask the AgentDash platform broker to evaluate a structured platform request.
- `parent`: send review or completion material back to the parent session.
- `sub`: dispatch work to a companion agent or session.

Read `references/human-interaction.md` for approval, free-form question, and notification examples.

## Payload Types

Use registered payload types when the intent matches a known protocol:

- `task` expects `completion`.
- `review` expects `resolution`.
- `approval` expects `decision`.
- `notification` expects no response.
- `capability_grant_request` expects `capability_grant_result`.

For the target/type/required-field matrix and examples, read `references/payload-envelope.md`.

## Capability Grants

Use `target: "platform"` with `payload.type: "capability_grant_request"` when the session needs a temporary tool or MCP capability that is not currently callable.

The companion response is only a conversation receipt. The platform grant record and applied `RuntimeCapabilityTransition` are the authority. For required fields and result shape, read `references/capability-grant-request.md`.

## Task Plan Tools

Task tools operate on run-scoped Task plan facts. Create, update, assignment, review, and done transitions go through the LifecycleRun Task command surface because Task identity belongs to the active run plan, while Story only reads a projection of related run tasks.

Task tool artifacts should be reported as `artifact_refs` or SubjectExecution-linked paths. Runtime evidence lives in Lifecycle / SubjectExecution projections, so companion completion payloads can point at files or durable execution records without turning Task plan DTOs into artifact stores.

## Workflow Script Preflight

Use workflow script preflight when a model or companion drafts a dynamic orchestration script for a multi-agent workflow, such as research fanout, review/approval pipelines, local capability effects, or generated runbooks that need inspection before runtime execution.

- Treat workflow scripts as restricted Rhai builder scripts that produce a builder document.
- Inspect diagnostics, plan preview, and capability summary before asking for approval.
- Keep approval, launch, and saved-definition work on the dedicated platform commands when those commands are available.

For the supported builder syntax, request/response shape, and compilation semantics, read `references/workflow-script-preflight.md`.

## Response Adoption

After receiving a response, continue from the structured result:

- Use `status` to decide whether to proceed, ask again, or abandon the path.
- Use `summary`, `findings`, `follow_ups`, and `artifact_refs` when returning work to a parent session.
- Treat capability grant result statuses as broker outcomes, then wait for capability delta or tool schema delta before calling newly granted tools.

Read `references/response-adoption.md` for response shapes.
