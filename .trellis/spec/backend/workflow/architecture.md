# Workflow and Agent Runtime Boundary

## Role

Workflow owns plan/orchestration/node/attempt/product evidence. concrete Agent owns
conversation/context/effect 与 terminal history；Runtime 只在当前进程协调两端。二者通过
Lifecycle run/agent/frame、source association 与 concrete operation coordinate关联。

## Invariants

- Workflow dispatch 先创建或复用 LifecycleRun、LifecycleAgent、AgentFrame，再调用
  `AgentRunProductInputDeliveryPort`；不创建执行 session 或具体 Agent client。
- Runtime node可以保存 `runtime_thread_id` / `runtime_operation_id` 作为下游 evidence；执行
  状态从 concrete Agent observation得到。
- AgentFrame 是 immutable Business Surface input；`AgentRunRuntimeBinding` 是产品到 Runtime 的唯一执行锚点。
- node completion由 concrete Agent terminal snapshot/event驱动并幂等写入orchestration journal。
- Gate/Companion follow-up保存自身等待事实，并以稳定source identity materialize AgentRun
  Mailbox message；下游 evidence分别记录Mailbox message与concrete operation coordinate。
- Workflow repository不读取Agent/Runtime内部表；跨边界查询使用facade/typed association ports。

## Validation

| Condition | Result |
| --- | --- |
| node 无 run/agent/frame coordinate | dispatch rejected |
| Runtime command unavailable | node remains blocked/diagnostic; no fallback executor |
| duplicate terminal evidence | same node attempt converges once |
| stale operation/thread coordinate | ignore/fence and refresh canonical binding |

## Tests Required

- dispatch → Channel/Product input delivery → Mailbox message → concrete operation evidence集成测试。
- duplicate/late Agent terminal does not advance node twice。
- workflow crates contain no Driver/vendor/legacy session repository dependency。
