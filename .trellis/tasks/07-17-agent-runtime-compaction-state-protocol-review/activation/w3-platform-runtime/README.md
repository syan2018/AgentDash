# W3 Platform Runtime S5 activation component

本目录冻结基于 `ed1a7d95aa9c4d10feda5cbed29cdb3c4bad02a7` 的 Platform
Runtime 激活组件。组件已经把四个 Platform library root 切到最终 Managed Runtime
Contract / Runtime transaction / Complete Agent Host / Runtime Wire revision 4，并删除这些
crate 内的 Driver、RuntimeJournal、Context Activation 与 revision 3 实现。

该提交不是可单独落到稳定分支的 checkpoint。`manifest.json` 精确列出 33 个仍由 W2、
W6、W7、W8 持有的 production consumer；它们必须在同一个 S5 staging 集成中完成切换。
本组件不修改 root Cargo/lockfile、正式 migration、production composition 或 canonical
generated artifacts。

## Runtime durable authority

`ManagedRuntimeStateRepository` 的一次 `ManagedRuntimeStateCommit` 是一个 Runtime
transaction。admission 读取已提交 projection revision 与 command availability；同一次
CAS 原子写入 operation、idempotency、pending effect intent、projection、typed change
与 outbox。外部 Complete Agent 调用只能发生在该 durable intent 提交之后。

`CompleteAgentStateRepository` 保存 source-normalized projection 与 source change
reconcile 证据。W8 的 PostgreSQL adapter 必须在最终 Runtime schema 中同时实现它和
`ManagedRuntimeStateRepository`，并把 source reconcile 产生的 Managed Runtime
projection/change 纳入 Runtime transaction。

## Host durable authority

`CompleteAgentHostRepository` 是 service instance、exact Runtime offer、placement、
binding、source coordinate、generation、effect、inspection、lease 与 lease epoch 的
唯一持久化端口。`CompleteAgentServiceRegistry` 只解析当前进程可调用句柄，不持有业务
事实。

一次 `CompleteAgentHostCommit` 是一个 Host transaction。adapter 必须锁定 revision，
整体校验 descriptor/offer/placement、binding/source/generation、effect/attempt evidence
与 lease fence，再原子推进 revision。exact committed fact graph replay 幂等返回；不同
graph 的 stale revision 返回 typed conflict。

## S5 集成顺序

1. 应用本 Platform Runtime activation component。
2. 应用 W2 Dash/Core 与 Native Complete Agent activation。
3. 应用 W6 Codex/Remote Complete Agent activation。
4. 应用 W7 Product/Protocol Managed Runtime callers。
5. 由 W8 增加唯一 migration、PostgreSQL repositories、root Cargo/lock 和 production
   composition。
6. 只运行一次 canonical generators。
7. 按 manifest 执行 33 个 consumer、legacy negative search 与 full-workspace gates。

完整表、事务约束、live signatures、语义矩阵、逐文件 consumer action 和验证命令都以
`manifest.json` 为准。
