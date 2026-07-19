# W7 Product durable persistence contract

## 归属

Product 定义 `AgentRunForkSagaRepository`、`CompanionFreshSagaRepository` 与状态机语义。
W8 独占正式 migration、PostgreSQL adapter 和 production composition。该合同不新增历史
migration，也不允许 Product 通过内存 repository 激活生产路径。

## AgentRun Fork transaction

`agent_run_fork_saga` 以 `request_id` 为唯一请求键，以 `version` 做 CAS。持久行必须覆盖：

- immutable parent coordinate、exact `through_turn_id` 和完整预分配 child
  `agent_run_id/run_id/agent_id/frame_id/presentation_thread_id`；
- 当前 phase、同一 `AgentRunForkOperationIdentity` 的 durable dispatch marker；
- Runtime/Agent child coordinate、Host binding、exact child history digest；
- admission/fork/provision/activation receipts、Product graph commit revision；
- terminal failure 或保留 known child coordinate 的 `Lost`，二者互斥。

请求 materialize 必须在一个事务中声明唯一 request 并保留完整 child identity。Runtime side
effect 前先提交 dispatch marker；响应未知时只 inspect 同一 identity。Product graph 与
`ProductGraphCommitted` saga revision 通过
`AgentRunForkSagaRepository::commit_product_graph` 在一个事务提交。activation 只能发生在
该事务之后。

`PreparedAgentRunForkGraph` 是可序列化、构造后只读的 transaction payload，完整携带
`LifecycleRun`、`LifecycleAgent`、`AgentFrame`、`AgentRunLineage`、presentation identity、
Runtime child、Host binding 与 history digest。payload digest 覆盖全部 immutable rows。
repository commit 同时接收 expected saga version、已经转移到
`ProductGraphCommitted` 的 saga 和 prepared graph：CAS 冲突时二者均不可见；成功时二者
同时可见；相同 request/payload 重放返回已提交 revision；同 request 的不同 payload
返回冲突。

## Fresh Companion transaction

`companion_fresh_saga` 以 `request_id` 为唯一请求键，以 `version` 做 CAS。持久行必须覆盖：

- immutable create/activation/first-input effect IDs；
- typed initial context package、contribution provenance、最低 fidelity 要求；
- durable effect dispatch identity、child coordinate、context application evidence；
- create/activation/first-input receipts 和 terminal Lost。

每个 effect 都先持久化 dispatch identity，再执行；重启只 inspect 同一 effect ID。只有
package ID、digest、每项 fidelity、renderer/materialized digest 全部通过验证后才能
activation；只有 activation receipt 已持久化后才能提交 first input。first-input effect ID
唯一，因此崩溃恢复不能产生第二次 SubmitInput。

## W8 constraints

- 两张 saga 表的 request key、effect identity 与 CAS revision 必须有数据库约束。
- graph rows、lineage、frame 与 saga graph-commit revision 必须在同一个 PostgreSQL
  transaction 中提交。
- known child 的 Lost 不做 delete compensation，也不允许重新 fork/create。
- 正式 migration 与 repository adapter 必须和 AppState constructor、六个 Product caller
  以及 canonical generated contract 一次激活。
