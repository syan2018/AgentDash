# Capability Grant Request

Use this payload through `target: "platform"` when the session needs a temporary capability.

```json
{
  "target": "platform",
  "wait": true,
  "payload": {
    "type": "capability_grant_request",
    "requested_paths": ["workflow_management::upsert_lifecycle_tool"],
    "reason": "需要更新当前 Project 的 lifecycle 定义",
    "scope": "session",
    "ttl_seconds": 3600,
    "interaction_hint": "Agent 请求临时获得 workflow lifecycle 更新能力。"
  }
}
```

## Requested Paths

`requested_paths` uses `ToolCapabilityPath` strings:

- `workflow_management`
- `workflow_management::upsert_lifecycle_tool`
- `mcp:code_analyzer`
- `mcp:code_analyzer::scan`

Capability-level paths request the whole capability. Tool-level paths request a single tool under that capability.

## Broker Flow

The platform broker maps the request into permission/grant handling:

1. Validate owner and visibility boundaries.
2. Validate requested capability paths.
3. Decide whether policy can approve, reject, or requires user approval.
4. On approval, compile capability declarations/effects.
5. Apply through `RuntimeCapabilityTransition`.
6. Emit capability state and tool schema delta events.

Companion result payloads keep the conversation coherent; they are not the authority for tool access.

## Result Shape

```json
{
  "type": "capability_grant_result",
  "status": "approved",
  "summary": "用户已批准临时能力申请",
  "granted_paths": ["workflow_management::upsert_lifecycle_tool"]
}
```

Use `status: "rejected"` with `rejected_paths` when the request is denied.
