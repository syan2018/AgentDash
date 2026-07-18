# W3 Platform Runtime activation input

本目录冻结 Platform Runtime bundle 在 S5 原子切换中交给 W8 的输入。当前代码只建立
target-lane contract、Host repository seam 和可复现的行为证据，不改变 production
composition，也不新增正式 migration。

## Host durable authority

`CompleteAgentHostRepository` 是 service instance、offer、binding、source coordinate、
generation、effect、inspection、lease 与 lease epoch 的唯一持久化端口。Host 的
`CompleteAgentServiceRegistry` 只解析当前进程可调用句柄；service descriptor 与所有
coordination facts 均从 repository 恢复。Host 本身不持有锁或内存 map。

一次 `CompleteAgentHostCommit` 是一个 Host transaction：

- adapter 必须锁定并比较 `CompleteAgentHostRevision`；
- exact committed fact graph replay 幂等返回当前 snapshot；
- revision 相同的不同 graph 原子提交并推进 revision；
- stale revision 的不同 graph 返回 typed `Conflict`；
- 当前 revision 到候选 revision 的 descriptor/offer、binding/source/generation、
  effect identity/evidence、lease token/epoch/expiry 约束在提交前整体校验；
- effect intent 在调用 Complete Agent 前提交，receipt/inspection 在调用后以
  当前 durable binding generation 与 active lease owner/token/epoch/expiry 再次 fence；
- revoke terminal receipt 与 binding applied surface 清理在同一 Host transaction
  结算，重启 replay 仍须完成未结清的 binding 清理。

W8 PostgreSQL adapter 与唯一 final migration 必须共同实现 `manifest.json` 冻结的表、
约束与事务语义；不得通过 Host 内存 map、Runtime repository、兼容表或双写补造事实。
`agent_runtime_host_revision` 是一次 Host transaction 的单调 CAS token，不承载业务事实。

## S5 activation order

1. 应用最终 Runtime Contract、Complete Agent Service API 与 Host repository shape。
2. 应用 Dash/Core physical component 与 final consumers。
3. 注册 Native、Codex、Remote Complete Agent services。
4. 激活 Product Runtime callers 与 canonical generated contracts。
5. W8 增加 PostgreSQL repositories、唯一 migration 和 production composition。
6. 删除 legacy driver、journal、context activation、旧 wire revision 与生成根。
7. 执行 manifest 中的 dependency、production-route、generator 与 deletion gates。

`CompleteAgentHost` 与 `CompleteAgentStateReconciler` 必须由 production composition
显式注入最终 PostgreSQL repositories；没有 repository 时 composition 失败，不存在
默认内存实现或 fallback。状态型 repository/registry 测试实现只存在于私有测试夹具，
production library 只公开持久化端口与纯 transaction validator。
