# Canvas Interaction State

Interaction state 是 instance 的 canonical shared state，不是 DOM snapshot 或 renderer diagnostics。

```ts
const current = await window.agentdash.interaction.getState();
const command = /* 从 operations.list/describe 取得 exact OperationRef */;

await window.agentdash.interaction.dispatch(
  command,
  { selected_id: "row-17" },
  current.state_revision,
);
```

## 规则

- 用 declared command/event 保存选择、表单、筛选和近期语义事件。
- `dispatch`/`emit` 必须传 exact OperationRef 与 current expected revision。
- revision conflict 后重新读取 state 和 descriptor，再决定是否重放用户意图。
- Agent 侧只读取 definition `agent_projection` allowlist 的 state。
- 不从 DOM 推断业务状态，不把 renderer ready/error/viewport 写入 canonical state。
- ResourceSlot binding 与 state 独立；binding 不隐式修改 state。
