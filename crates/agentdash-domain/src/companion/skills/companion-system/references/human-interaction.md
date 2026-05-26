# Human Interaction

Use `target: "human"` when the agent needs the current user to provide judgment or missing information.

Approval:

```json
{
  "target": "human",
  "wait": true,
  "payload": {
    "type": "approval",
    "prompt": "是否允许本轮修改 Trellis 任务文档？",
    "options": ["允许", "不允许"]
  }
}
```

Free-form answer:

```json
{
  "target": "human",
  "wait": true,
  "payload": {
    "type": "approval",
    "prompt": "请补充这个任务的最小验收标准。"
  }
}
```

Notification:

```json
{
  "target": "human",
  "wait": false,
  "payload": {
    "type": "notification",
    "message": "我已把持久化模型拆成后续任务。"
  }
}
```

Use `wait: true` when the next action depends on the response. Use `wait: false` when the response can arrive as later session context.
