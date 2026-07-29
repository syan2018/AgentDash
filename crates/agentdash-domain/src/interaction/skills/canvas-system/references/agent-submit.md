# Canvas Submit To Agent

Canvas UI 只能在用户明确点击或提交后，把反馈交给当前 attached AgentRun：

```ts
await window.agentdash.agent.submit({
  text: "分析当前选择并给出下一步建议",
  client_command_id: crypto.randomUUID(),
  include_interaction_state: true,
  include_render_observation: false,
});
```

也可提交 generated `AgentInputContent[]`：

```ts
await window.agentdash.agent.submit({
  input: [
    { kind: "text", text: "继续处理" },
    { kind: "structured", schema: "canvas.selection.v1", value: { id: "row-17" } },
  ],
  client_command_id: crypto.randomUUID(),
});
```

## 规则

- 提供非空 `text` 或 canonical `input`。
- `client_command_id` 是幂等键；同一用户动作重试时复用它。
- 只在模型确实需要时设置 `include_interaction_state` 或
  `include_render_observation`；host 从 canonical backend facts 读取，不信任 iframe 自报快照。
- host 绑定当前 presentation attachment，iframe 不提交 run/agent identity。
- Product input delivery 根据 concrete Agent 的 authoritative active turn 决定 submit/steer；
  Canvas 不自行伪造运行状态或离线队列。
- standalone preview 保持其它功能可用，但 `agent.submit` 明确返回 mailbox unavailable。
- state 与 renderer observation 默认不进入模型输入；显式 include 后由 host 追加 typed structured
  input。
