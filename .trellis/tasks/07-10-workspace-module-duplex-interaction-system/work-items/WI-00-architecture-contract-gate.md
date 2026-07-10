# WI-00 Architecture / Contract Gate

Status: planned

Depends On: 已确认产品决策与现状证据

## Scope

- 把 decision ledger 中已确认的 OperationScript、Interaction、Extension、Canvas 替换和 RuntimeSession 决策写入权威 specs。
- 写出 canonical Operation、`RuntimeInvocationEnvelope`、OperationScript engine port、Interaction command/state transition、definition/instance ownership 与 artifact pin contracts。
- 固定 V1 discriminator、module/presentation identity、SourceBundle/lineage/resource binding、`state_patch_v1` 与 OperationEffectIntent。
- 对账 RuntimeGateway、Session、Capability、VFS、Frontend 和 cross-layer specs。
- 建立旧 Session-bound / Canvas-specific contract 到目标 contract 的删除或替换矩阵。

## Exit Criteria

- 相关 spec 不再把 RuntimeSession 当作 Canvas/Extension authority。
- spec 使用 async executor + `rhai_v1`、versioned platform state transition、exact source/artifact pinning 和最终 V1 Interaction schema 作为唯一目标合同。
- WI-01 至 WI-09 的依赖、write set 和验收条件可从合同直接推导。
- 父任务 PRD/design/implement 完成 convergence review，并获得用户最终规划批准。

## Validation

- `rg` 核对目标 spec 中 RuntimeGateway、OperationScript、Interaction、Canvas 与 Extension 术语。
- `task.py validate` 核对 implement/check JSONL。
- 用户最终 planning review。
