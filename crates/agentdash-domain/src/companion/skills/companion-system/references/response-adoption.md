# Response Adoption

`companion_respond` always names the `request_id` and sends an object payload.

```json
{
  "request_id": "dispatch-1",
  "payload": {
    "type": "completion",
    "status": "completed",
    "summary": "子任务完成",
    "findings": ["需要补一个 UI renderer"],
    "follow_ups": ["运行 app-web typecheck"]
  }
}
```

## Parent And Sub Sessions

When responding to a parent session, include enough structure for the parent to adopt the result:

- `summary`: concise conclusion.
- `status`: `completed`, `approved`, `rejected`, or `needs_revision`.
- `findings`: concrete observations.
- `follow_ups`: actions the parent should consider.
- `artifact_refs`: files, lifecycle artifact paths, SubjectExecution records, or other durable records. Task plan items reference these externally because Task facts describe plan progress, not runtime evidence.

## Human And Platform Responses

Human approval generally returns `decision`.

```json
{
  "type": "decision",
  "status": "approved",
  "choice": "继续",
  "summary": "用户同意继续"
}
```

Capability grant broker responses return `capability_grant_result`.

```json
{
  "type": "capability_grant_result",
  "status": "rejected",
  "summary": "用户拒绝授予 workflow_management"
}
```

For capability grants, call newly granted tools only after the runtime emits a capability state update or tool schema delta that shows the tool is available.
