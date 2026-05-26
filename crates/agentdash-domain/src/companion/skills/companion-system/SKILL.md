---
name: companion-system
description: AgentDash companion 通用交互协议手册。用于通过 companion_request/companion_respond 发起 human、platform、parent、sub/session 交互，申请临时能力扩展，采用结构化 payload，并把授权事实汇入平台权限与 Capability runtime。
---

# Companion System

Use this skill when a session has companion tools and needs structured cross-subject interaction.

## Core Model

- `companion_request` is the active interaction entrypoint. It carries intent and routing metadata.
- `companion_respond` returns a structured response to a known `request_id`.
- `payload` is always a JSON object. Use `payload.type` for registered protocols.
- Companion events record interaction continuity. Permission, grant, and runtime capability state remain the authority for tool access.

## Target Selection

- `human`: ask the current user for approval, choice, missing information, or scope confirmation.
- `platform`: ask the AgentDash platform broker to evaluate a structured platform request.
- `parent`: send review or completion material back to the parent session.
- `sub`: dispatch work to a companion agent or session.

## Payload Types

Use registered payload types when the intent matches a known protocol:

- `task` expects `completion`.
- `review` expects `resolution`.
- `approval` expects `decision`.
- `notification` expects no response.
- `capability_grant_request` expects `capability_grant_result`.

For field-level examples, read `references/payload-envelope.md`.

## Capability Grants

Use `target: "platform"` with `payload.type: "capability_grant_request"` when the session needs a temporary tool or MCP capability that is not currently callable.

Required fields:

- `requested_paths`: non-empty array of `ToolCapabilityPath` strings.
- `reason`: concise operational reason.
- `scope`: `turn`, `session`, or `workflow_step`.

Optional fields:

- `ttl_seconds`: positive integer lifetime.
- `interaction_hint`: approval text for the user.

The companion response is only a conversation receipt. The platform grant record and applied `RuntimeCapabilityTransition` are the authority. For details, read `references/capability-grant-request.md`.

## Response Adoption

After receiving a response, continue from the structured result:

- Use `status` to decide whether to proceed, ask again, or abandon the path.
- Use `summary`, `findings`, `follow_ups`, and `artifact_refs` when returning work to a parent session.
- Treat capability grant result statuses as broker outcomes, then wait for capability delta or tool schema delta before calling newly granted tools.

Read `references/response-adoption.md` for response shapes.
