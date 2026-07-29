# Canvas Runtime Operations

Canvas 只能调用 `operations.list()` 返回、并经 `operations.describe()` 确认的 exact
OperationRef：

```ts
const operations = await window.agentdash.operations.list();
const operation = operations.find((item) => item.operation_ref.operation_key === "refresh");
if (!operation) throw new Error("refresh Operation 当前不可用");

const result = await window.agentdash.operations.invoke(
  operation.operation_ref,
  { limit: 10 },
  { idempotencyKey: crypto.randomUUID() },
);
```

## 规则

- 只从显式点击、提交或刷新动作调用 mutation/external Operation。
- 不按 operation key 字符串自行构造 provider、scope 或 authority。
- 不发送 token、backend ID、MCP transport、relay command、绝对路径或任意 HTTP 请求。
- 每次调用由可信 host 重新进入 OperationGateway，重新校验 current actor surface、schema、
  readiness、authority、deadline 与 cancellation。
- MCP tool、Extension protocol channel/backend service 只有在当前 catalog 投影为 exact
  Operation 时才可调用；不存在旧字符串 bridge 旁路。
- 展示结构化失败，权限撤销或 stale descriptor 后重新 list/describe。
