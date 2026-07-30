# Canvas Runtime Operations

当 renderer 需要直接调用已投影 Operation 时，先从 `operations.list()` 取得当前 runtime
catalog，再通过 `operations.describe()` 确认 exact OperationRef：

```ts
const operations = await window.agentdash.operations.list();
const operation = operations.find((item) => item.operation_ref.operation_key === "load_summary");
if (!operation) throw new Error("load_summary Operation 当前不可用");

const result = await window.agentdash.operations.invoke(
  operation.operation_ref,
  { limit: 10 },
  { idempotencyKey: crypto.randomUUID() },
);
```

## 规则

- Canvas 自身定义的按钮、刷新与表单提交优先建模为 `canvas.json.actions`，由
  `window.agentdash.actions.invoke(...)` 触发。
- `operations.list()` 只是 renderer 当前 runtime catalog，不等同于 Agent 当前完整工具面。
- 只从显式点击、提交或刷新动作调用 mutation/external Operation。
- 不按 operation key 字符串自行构造 provider、scope 或 authority。
- 不发送 token、backend ID、MCP transport、relay command、绝对路径或任意 HTTP 请求。
- 每次调用由可信 host 重新进入 OperationGateway，重新校验 current actor surface、schema、
  readiness、authority、deadline 与 cancellation。
- MCP tool、Extension protocol channel/backend service 只有在当前 catalog 投影为 exact
  Operation 时才可调用；不存在旧字符串 bridge 旁路。
- 展示结构化失败，权限撤销或 stale descriptor 后重新 list/describe。
