# OperationScript 组合

## 使用边界

仅把 `operation_script` 用于有界、ephemeral 的即时组合。它直接绑定当前 actor Operation
surface，可以跨 `builtin:*`、Canvas、Interaction 与 Extension providers 调用，不属于某个
Workspace Module lifecycle provider。

需要 durable retry、recovery、human gate、跨 session 状态或可恢复多步副作用时使用 Workflow。
OperationScript 不自动成为 Interaction state command，也不承诺整段脚本可安全 replay。

## Exact Operation identity

先 list/describe 所需 modules，再把 descriptor 返回的四段 identity 写成：

```text
namespace:provider_key:operation_key:v<contract_version>
```

不要猜测、缓存或自行构造 OperationRef。surface、authority 或 descriptor 改变后重新 describe。

## Rhai host API

工具参数只有 `source` 与可选 `input`。使用：

- `ops.invoke(exact_operation_ref, input)`：执行有依赖的单次或顺序调用。
- `ops.invoke_all([{operation, input}, ...])`：并发执行互不依赖的调用，结果保持输入顺序。
- 全局 `input`：工具调用传入的 JSON。
- 脚本最后一个表达式：工具返回值。

示例：

```json
{
  "source": "let results = ops.invoke_all([#{ operation: \"platform:vfs:mounts_list:v1\", input: #{} }, #{ operation: \"platform:task:task_read:v1\", input: #{ mode: input.task_mode } }]); #{ mounts: results[0], task: results[1] }",
  "input": {
    "task_mode": "overview"
  }
}
```

只有最新 describe 确实返回示例中的 refs 时才可使用这些字符串。

## 执行与失败

服务端固定 Rhai dialect、host API、limits、allowed manifest、principal、scope 与 authority。
脚本执行前会绑定 current surface，每个 nested call 仍重新进入 OperationGateway admission。

检查返回值与 call evidence。出现 partial call 或 `outcome_unknown` 时，不要假定整段脚本没有产生
副作用，也不要盲目重放；按每个 Operation 的 effect 与 replay policy 决定后续动作。
