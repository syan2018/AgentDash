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

source-normalized projection、source identity mapping、source change，以及由该 observation
导出的 Managed Runtime projection/change/outbox 都属于同一个
`ManagedRuntimeStateCommit`。normalize/project 只准备候选事实，不存在独立 source
repository 提交。每条 source change 都有且只有一条 `SourceObservationApplied` typed
change 与对应 outbox，固化 source、projection revision、observation、source revision、
cursor digest 和 changed sections。normalized payload 即使没有改变 projection，也会以
空 `changed_sections` 保留因果 observation；只有真实改变才追加具体 projection delta。
CAS 或持久化失败时整组事实均不推进。只有已提交的可信 cursor 才能
消费连续有序 change page；首次同步、断档或 partial page 都回到 snapshot authority，
而 snapshot contract 尚未提供可信 cursor 时将 `source_cursor` 保持为 `null`。

## Host durable authority

`CompleteAgentHostRepository` 是 service instance、exact Runtime offer、placement、
binding、source coordinate、generation、effect、inspection、lease 与 lease epoch 的
唯一持久化端口。`CompleteAgentServiceRegistry` 只解析当前进程可调用句柄，不持有业务
事实。

一次 `CompleteAgentHostCommit` 是一个 Host transaction。adapter 必须锁定 revision，
整体校验 descriptor/offer/placement、binding/source/generation、effect/attempt evidence
与 lease fence，再原子推进 revision。exact committed fact graph replay 幂等返回；不同
graph 的 stale revision 返回 typed conflict。

callback route 与 tombstone 属于 `CompleteAgentHostFacts`：callback-bound surface
进入 Applied/Available 时必须在同一个 Host commit 插入唯一 route，revoke/lost 时在同一个
Host commit 保留不可变 route fence 并追加 tombstone；没有 callback contribution 的
surface 不产生 route。`CompleteAgentCallbackRepository` 只持有 reservation 与 outcome，
reservation 固化已提交 route 的 generation 与 bound-surface digest，并由 PostgreSQL
compound reference 约束到仍 active 的 Host route。每次 Tool/Hook 在调用 platform handler
前先 CAS 写入 `Pending` reservation，随后用第二次 CAS 结算 typed outcome。进程重启后
`Settled` 精确 replay；route tombstone 则优先拒绝旧 generation callback。
`Pending`、`InspectionRequired` 与 `Unknown` 都禁止自动重执行，只能通过公开 inspect
与显式 reconcile/settle 收敛。

## S5 集成顺序

1. 应用本 Platform Runtime activation component。
2. 应用 W2 Dash/Core 与 Native Complete Agent activation。
3. 应用 W6 Codex/Remote Complete Agent activation。
4. 应用 W7 Product/Protocol Managed Runtime callers。
5. 由 W8 增加唯一 migration、PostgreSQL Runtime/Host/callback repositories、root
   Cargo/lock 和 production composition。
6. 只运行一次 canonical generators。
7. 按 manifest 执行 33 个 consumer、legacy negative search 与 full-workspace gates。

完整表、事务约束、live signatures、语义矩阵、逐文件 consumer action 和验证命令都以
`manifest.json` 为准。
