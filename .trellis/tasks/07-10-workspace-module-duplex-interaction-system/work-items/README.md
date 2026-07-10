# Workspace Module 双工交互 Work Items

Parent task: `.trellis/tasks/07-10-workspace-module-duplex-interaction-system`

本目录追踪父任务内部实施项。根 `prd.md`、`design.md`、`implement.md` 仍是需求、设计与全局执行顺序的权威来源。

## Status

`planned -> implementing -> checking -> ready_for_integration -> done`

设计或实现证据表明目标合同需要调整时回到 `planned`，先更新 decision ledger 与父任务规划再继续。

## Tracker

| ID | File | Status | Depends On | Last Evidence |
| --- | --- | --- | --- | --- |
| WI-00 | `WI-00-architecture-contract-gate.md` | ready_for_integration | 已确认产品决策 | 权威 specs 已固定 Interaction V1、OperationScript async bridge、identity、SourceBundle、state/effect 与 RuntimeSession removal 边界 |
| WI-01 | `WI-01-runtime-gateway-operation-core.md` | planned | WI-00 | 现行 Session-bound actor/context 与重复 admission 路径已核实 |
| WI-02 | `WI-02-user-workshop-canvas-standalone.md` | planned | WI-01 | Project Canvas 仍请求已删除 endpoint；目标 bridge 已固定 |
| WI-03 | `WI-03-operation-script.md` | planned | WI-01 | Rhai 1.24 同步 evaluator 通过 bounded worker/async bridge 承载 `rhai_v1` |
| WI-04 | `WI-04-interaction-domain.md` | planned | WI-00 | owner/lifetime、`state_patch_v1`、EffectIntent、SourceBundle/CAS 已确认 |
| WI-05 | `WI-05-canvas-interaction-migration.md` | planned | WI-02、WI-04 | Canvas 无需旧数据 backfill；新模型完整承接 distribution/VFS/binding/promotion |
| WI-06 | `WI-06-extension-component-runtime.md` | planned | WI-02、WI-04、Channel WI-01 | Component + Operation、exact artifact pin 已确认 |
| WI-07 | `WI-07-workspace-module-convergence.md` | planned | WI-01、WI-03 至 WI-06 | Workspace Module 目标为 Agent projection |
| WI-08 | `WI-08-workflow-channel-integration.md` | planned | WI-03、WI-04、WI-06、Channel ref/admission | Workflow 复用 executor；Channel 只承载 attention refs |
| WI-09 | `WI-09-runtime-session-cleanup.md` | planned | WI-02、WI-05、WI-06、AgentRun adapter | 现有直接 Session context 使用需要分类并删除目标耦合 |
| WI-10 | `WI-10-integration-spec-verification.md` | planned | WI-01 至 WI-09 | 父任务最终全量 gate |

## Decision Ledger

见 [decisions.md](./decisions.md)。产品决策已经收敛；实施中只在证据会改变产品语义时重新打开规划评审。

## Update Contract

- 开始工作项前记录实际 write set 和依赖是否满足。
- 完成实现后记录修改文件、focused checks 与剩余风险。
- targeted check 通过只能进入 `ready_for_integration`；WI-10 全量检查后才进入 `done`。
- tracker 状态不替代 PRD acceptance criteria、spec convergence 或代码静态检查。
