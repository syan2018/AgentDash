# Workflow and Agent Runtime Boundary

## Role

Workflow owns plan/orchestration/node/attempt/product evidence. Agent Runtime owns Thread/Turn/Item/Interaction、operation journal、context 与 terminal lifecycle。二者通过 Lifecycle run/agent/frame 与 canonical Runtime thread/operation coordinates关联。

## Invariants

- Workflow dispatch 先创建或复用 LifecycleRun、LifecycleAgent、AgentFrame，再调用 `AgentRunProductDelivery`；不创建执行 session 或 Driver client。
- Runtime node 保存 `runtime_thread_id` / `runtime_operation_id` evidence；不能复制 Runtime status machine。
- AgentFrame 是 immutable Business Surface input；`AgentRunRuntimeBinding` 是产品到 Runtime 的唯一执行锚点。
- node completion 由 canonical Runtime terminal event/snapshot 驱动并幂等写入 orchestration journal。
- Gate/companion follow-up 进入 durable AgentRun mailbox；accepted result 只引用 canonical Runtime operation。
- Workflow repository 不读取 Runtime 内部表；跨边界查询使用 facade/typed binding ports。

## Validation

| Condition | Result |
| --- | --- |
| node 无 run/agent/frame coordinate | dispatch rejected |
| Runtime command unavailable | node remains blocked/diagnostic; no fallback executor |
| duplicate terminal evidence | same node attempt converges once |
| stale operation/thread coordinate | ignore/fence and refresh canonical binding |

## Tests Required

- dispatch -> ProductDelivery -> operation evidence integration test。
- duplicate/late Runtime terminal does not advance node twice。
- workflow crates contain no Driver/vendor/legacy session repository dependency。
