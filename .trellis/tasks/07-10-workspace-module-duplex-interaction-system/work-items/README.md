# Workspace Module 双工交互 Work Items

Parent task: `.trellis/tasks/07-10-workspace-module-duplex-interaction-system`

本目录追踪父任务内部实施项。根 `prd.md`、`design.md`、`implement.md` 仍是需求、设计与全局执行顺序的权威来源。

## Status

`planned -> implementing -> checking -> ready_for_integration -> done`

设计或实现证据表明目标合同需要调整时回到 `planned`，先更新 decision ledger 与父任务规划再继续。

## Tracker

| ID | File | Status | Depends On | Last Evidence |
| --- | --- | --- | --- | --- |
| WI-00 | `WI-00-architecture-contract-gate.md` | done | 已确认产品决策 | canonical Operation、Interaction V1、Rhai 与 Extension 最终合同已进入 specs |
| WI-01 | `WI-01-runtime-gateway-operation-core.md` | done | WI-00 | exact provider catalog、可信 envelope、统一 execution core 与 MCP/Extension/Interaction provider |
| WI-02 | `WI-02-user-workshop-canvas-standalone.md` | done | WI-01 | Project-scoped OperationWorkshop、standalone Canvas/Extension host 与 server-resolved authority |
| WI-03 | `WI-03-operation-script.md` | done | WI-01 | bounded async Rhai、preflight/run、Agent/UserWorkshop/Workflow callers 与 scoped result ref |
| WI-04 | `WI-04-interaction-domain.md` | done | WI-00 | immutable revision、Instance、CAS command/event、presentation/lease、attachment 与 effect admission |
| WI-05 | `WI-05-canvas-interaction-migration.md` | done | WI-02、WI-04 | 0062 migration、旧 Canvas 删除、Interaction frontend、exact Extension promotion |
| WI-06 | `WI-06-extension-component-runtime.md` | done | WI-02、WI-04、Channel WI-01 | Component ABI、隔离 host、exact artifact 与 canonical Operation bridge |
| WI-07 | `WI-07-workspace-module-convergence.md` | done | WI-01、WI-03 至 WI-06 | Workspace Module 仅投影 AgentFrame attachment 与 exact Interaction Operations |
| WI-08 | `WI-08-workflow-channel-integration.md` | done | WI-03、WI-04、WI-06、Channel ref/admission | Workflow OperationScript node 与 Channel typed attention/admission 完成 |
| WI-09 | `WI-09-runtime-session-cleanup.md` | done | WI-02、WI-05、WI-06、AgentRun adapter | Session-bound action gateway 删除，RuntimeSession 仅保留 delivery/trace evidence |
| WI-10 | `WI-10-integration-spec-verification.md` | done | WI-01 至 WI-09 | targeted/full gates、规范收敛、残留扫描与仓库既有失败分类完成 |

## Decision Ledger

见 [decisions.md](./decisions.md)。产品决策已经收敛；实施中只在证据会改变产品语义时重新打开规划评审。

## Update Contract

- 开始工作项前记录实际 write set 和依赖是否满足。
- 完成实现后记录修改文件、focused checks 与剩余风险。
- targeted check 通过只能进入 `ready_for_integration`；WI-10 全量检查后才进入 `done`。
- tracker 状态不替代 PRD acceptance criteria、spec convergence 或代码静态检查。
