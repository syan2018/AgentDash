# Companion Payload Envelope

Companion tool arguments use a stable envelope:

```json
{
  "target": "human",
  "wait": true,
  "payload": {
    "type": "approval",
    "message": "是否继续执行这个高风险操作？",
    "options": ["继续", "取消"]
  }
}
```

`payload` must be a JSON object. Registered `payload.type` values get role, required field, response type, and UI hint validation.
Request message bodies use `payload.message`. `payload.prompt` is not part of the companion request contract.

## Request Payload Matrix

| target | payload.type | Required fields | Response type | Purpose |
| --- | --- | --- | --- | --- |
| `sub` | `task` | `message` | `completion` | Dispatch work to a companion child agent. |
| `parent` | `review` | `message` | `resolution` | Ask the parent agent to review or decide. |
| `human` | `approval` | `message` | `decision` | Ask the user for a blocking choice or missing information. |
| `human` | `notification` | `message` | none | Notify the user without requiring a response. |
| `platform` | `capability_grant_request` | `requested_paths`, `reason`, `scope` | `capability_grant_result` | Ask the platform permission broker for temporary capability. |
| `platform` | `workflow_script_preflight` | `source_text` | none | Validate and preview a restricted Rhai workflow builder script. |

## Request Types

`task`:

```json
{
  "type": "task",
  "message": "审阅当前改动并返回风险点",
  "label": "reviewer",
  "context_mode": "compact"
}
```

`review`:

```json
{
  "type": "review",
  "message": "请主 session 判断这个方案是否可以进入实现"
}
```

`approval`:

```json
{
  "type": "approval",
  "message": "请选择下一步范围",
  "options": ["只做契约", "契约加 UI"]
}
```

`notification`:

```json
{
  "type": "notification",
  "message": "我已经把长期持久化拆到独立任务"
}
```

`capability_grant_request`:

```json
{
  "type": "capability_grant_request",
  "requested_paths": ["tools.fs.write"],
  "reason": "需要写入本轮实现文件",
  "scope": "turn",
  "ttl_seconds": 600
}
```

`workflow_script_preflight`:

```json
{
  "type": "workflow_script_preflight",
  "source_text": "workflow(#{ body: [] })",
  "args": { "topic": "orchestration" },
  "ctx": { "workspace": "demo" },
  "runtime_thread_id": "optional-thread-id"
}
```

## Response Types

`completion`:

```json
{
  "type": "completion",
  "status": "completed",
  "summary": "审阅完成",
  "findings": ["缺少边界校验"]
}
```

`resolution`:

```json
{
  "type": "resolution",
  "status": "approved",
  "summary": "方案可进入实现"
}
```

`decision`:

```json
{
  "type": "decision",
  "status": "approved",
  "choice": "契约加 UI",
  "summary": "按契约加 UI 推进"
}
```

`capability_grant_result`:

```json
{
  "type": "capability_grant_result",
  "status": "approved",
  "summary": "本轮 turn 已授予 tools.fs.write"
}
```
