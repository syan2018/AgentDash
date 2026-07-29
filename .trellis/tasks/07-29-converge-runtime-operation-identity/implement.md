# Implementation Plan

## 1. 先用删除测试锁定最终合同

- 为 Product receipt 增加测试：operation handle 等于 submitted Complete Agent public effect。
- 为 Agent/Runtime/canonical contracts 增加编译与 schema 断言，确认 execution projection 不再发布
  operation identity，Runtime view 不再有恒空 operations。
- 为 Dash command chain 增加恢复测试，证明无 lifecycle child effect ledger 时 A/B/C 仍可由
  command/dependency/history/lease 正确收敛。

## 2. 删除无 owner 的身份路径

- Product facade 删除独立 Runtime operation helper，receipt 显式包装 public effect。
- Agent service API 删除 active/queued/outcome/blocking 的 operation 字段。
- Runtime contract/wire/projector 删除 `AgentRuntimeOperation` 与 `operations`。
- Native canonical compaction item 删除 `operation_id`。
- 更新 Codex/Native adapter、generated Rust/TypeScript/schema 与 fixtures。

## 3. 精简 Dash lifecycle

- 删除 `DashLifecycle.effects`、`EffectOutcome`、`EffectSettlement`。
- 删除 `DashExecutionInspection` 及 `DashEffectInspection.execution`；保留 public effect receipt
  state/retryability。
- 删除 B/C child effect 派生、settlement 与相关 recovery 参数。
- 保留 Complete Agent source-level public effect record，让外层 effect 在完整 command chain terminal
  后收敛。
- 用 command 状态与 dependency 覆盖 failed/lost/blocked propagation。

## 4. 精简 compaction history

- 从 Queued/Started 删除 operation ID。
- 删除 `CompactionCheckpoint` 及相关 tool-pair/usage 聚合。
- 将 Applied 改为最小平铺事实，并集中实现 context revision 生成/验证。
- 调整 fold：SideEffectStarted 只约束 active recovery；Applied 本身足以证明副作用完成。
- 删除 snapshot/canonical projection 中无消费者的 compaction operation 字段。

## 5. 删除 persisted changes 镜像

- 从 `DashAgentStore` 删除 `changes`。
- change API 直接按 history revision 投影 entries。
- 删除 `ActiveTurnChanged`、ordinal cursor、changes digest 重建及对应 adapter 空分支。
- 用 reload/change-feed 测试确认顺序、cursor 和 canonical item 输出保持正确。

## 6. 删除 fake Product Rebind

- 删除 Product Agent command `Rebind` variant、HTTP mapping、generated contract 和测试。
- 验证 Host/Product convergence rebind 与 Runtime lifecycle evidence 不受影响。

## 7. 实现正式 typed migration

- 增加 repository schema version 与 startup migration gate。
- 建立旧 flat Applied、nested checkpoint、缺 operation 字段、带 operation 字段等真实 fixture。
- 将旧 repository 归一化为最终 history/lifecycle/store shape，重算 source/context/repository digest。
- 不合成 claim、时间、effect 或 terminal；不可证明的 active state fail-fast。
- 定点迁移 Routine、Channel、Gate 保存的旧 Product operation receipts。
- 通过 production loader 和完整 owner invariants 验证后原子写回。

## 8. 文档与验证

- 更新 `CONTEXT.md` 与相关 Trellis specs，只记录最终 owner 规则和单一 source 原因。
- 检索清理误导 helper、comment、fixture 与字段。
- 按影响面运行定向 Rust tests、migration tests、contract generation/check、frontend typecheck/tests。
- 共享脏工作区只格式化本任务拥有的文件，不触碰并行修改。

## Review Gates

- production decoder 无 default、旧 variant 双读或 fallback。
- 除 receipt 外，execution presentation 无 operation/effect identity。
- Dash 内部无 B/C effect ledger；public effect owner仍可 duplicate inspect/recover/terminalize。
- history 只有一份，Applied 只有 canonical 最小事实。
- migration 不伪造未知历史事实，并能打开已知失败会话 shape。
