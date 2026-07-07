# Response Adoption

`companion_respond` sends an object `payload`. Omit `reply_to` when the prompt lists a single active reply target; include the exact short selector only when the prompt lists multiple reply targets.

```json
{
  "payload": {
    "type": "completion",
    "status": "completed",
    "summary": "子任务完成",
    "findings": ["需要补一个 UI renderer"],
    "follow_ups": ["运行 app-web typecheck"]
  }
}
```

When a prompt lists multiple targets, keep the selector short and copy only the listed alias.

```json
{
  "reply_to": { "kind": "alias", "alias": "parent" },
  "payload": {
    "type": "completion",
    "status": "completed",
    "summary": "子任务完成"
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
