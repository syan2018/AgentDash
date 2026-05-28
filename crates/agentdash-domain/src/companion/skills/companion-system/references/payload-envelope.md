# Companion Payload Envelope

Companion tool arguments use a stable envelope:

```json
{
  "target": "human",
  "wait": true,
  "payload": {
    "type": "approval",
    "prompt": "是否继续执行这个高风险操作？",
    "options": ["继续", "取消"]
  }
}
```

`payload` must be a JSON object. Registered `payload.type` values get role, required field, response type, and UI hint validation.

## Request Types

`task`:

```json
{
  "type": "task",
  "prompt": "审阅当前改动并返回风险点",
  "label": "reviewer",
  "context_mode": "compact"
}
```

`review`:

```json
{
  "type": "review",
  "prompt": "请主 session 判断这个方案是否可以进入实现"
}
```

`approval`:

```json
{
  "type": "approval",
  "prompt": "请选择下一步范围",
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
