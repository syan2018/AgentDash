# 收敛启动恢复控制面架构

## Goal

评估并规划一次后续架构重构，把启动恢复、RuntimeSession terminal、AgentRun control-effect replay、mailbox wake admission 和 launch eligibility 中散落的顺序约束收敛为更深的控制面 module。目标不是重写业务语义，而是让恢复期“不创建新工作、先收敛事实、再执行副作用”的规则通过 interface 表达，减少后续同类 bug 的修复半径。

## Background

`startup-recovery-control-effect-replay` 修复已落地并提交为 `9bdf0bdfa`。该修复证明当前模型可以正确恢复坏库并让 `agentdash-server :3011/api/health -> 200`，但修复触及 28 个文件，暴露出一个架构信号：恢复控制面的关键不变量分散在多个调用点，而不是由一个深 module 承担。

已确认的耦合现象：

- 启动恢复顺序由 `AppState`、`reconcile::boot`、`background_workers`、`SessionRuntimeService::recover_interrupted_sessions` 与 `AgentRunControlEffectService` 共同拼装。
- control-effect replay 的 caller 需要理解 `DeliveryConvergence` 与 `TerminalSideEffects` 的执行顺序。
- mailbox wake 是否允许创建新 turn 由 companion、hook auto-resume、mailbox policy 和 scheduler 多处判断。
- `LaunchOrContinueTurn` 同时承载用户继续会话、companion parent resume、hook auto-resume、routine 投递等多种语义，caller 需要额外知道来源是否允许 terminal launch。
- terminal processor 的 `effect_mode` 需要沿 launch commit、connector start、runtime control、terminal boundary 多层穿透。

## Requirements

- R1. 产出一份架构评估，明确哪些耦合属于本任务，哪些应拆成后续任务；纳入标准必须基于同一个恢复/terminal side-effect/admission 不变量，而不是泛化 cleanup。
- R2. 设计一个启动恢复控制面深 module，例如 `StartupRecoveryPipeline` 或等价命名，把 session recovery、delivery convergence、terminal side-effect replay、gate fallback 和 diagnostics 的顺序收进单一 interface。
- R3. 设计 control-effect replay 的语义化 interface，避免 caller 直接拼 phase / limit；兼容入口必须保持总 batch 边界和 delivery-first 语义。
- R4. 设计 mailbox wake admission module，把 companion parent wake、hook auto-resume、user mailbox intake、scheduler launch eligibility 的 target-state decision 收敛为可测试 decision model。
- R5. 设计 launch intent / mailbox delivery 的语义拆分策略，识别 `LaunchOrContinueTurn` 中需要分离或显式标注 recovery safety 的来源。
- R6. 保持当前已修复行为不回退：坏库启动不栈溢出，terminal parent companion wake 对 terminal target 返回 no-op/skip，不创建 mailbox message。
- R7. 途中发现的类似耦合可以纳入任务范畴，但必须满足至少一条：
  - 共享恢复期禁止创建新 work 的不变量；
  - 共享 RuntimeSession execution state 与 AgentRun delivery binding 一致性判断；
  - 共享 terminal side-effect replay 顺序或 outbox admission 语义；
  - 共享 launch eligibility / mailbox wake decision。
- R8. 不纳入与该控制面无关的 clippy debt、普通模块拆分、命名整理、前端展示调整或数据库 schema 重排。

## Acceptance Criteria

- [ ] AC1. `research/coupling-inventory.md` 列出已观察耦合点、代码证据、纳入/排除判断和推荐拆分顺序。
- [ ] AC2. `design.md` 明确目标 module、interface、seam、decision model、adapter 变化和测试表面。
- [ ] AC3. `implement.md` 把工作拆成可独立验证的阶段；每阶段有风险、验证命令和回退点。
- [ ] AC4. 对恢复控制面、control-effect replay、mailbox wake admission 和 launch eligibility 的重构都有可执行验收标准。
- [ ] AC5. 规划必须要求启动烟测覆盖 embedded PostgreSQL 坏状态恢复路径，不能只依赖单元测试。
- [ ] AC6. 规划必须包含“类似耦合纳入规则”，并说明遇到范围外耦合时如何记录为后续任务。
- [ ] AC7. Sub-agent manifests `implement.jsonl` / `check.jsonl` 包含真实 spec/research 上下文，不保留创建任务时生成的示例行。

## Out Of Scope

- 不在本 task planning 阶段修改生产代码。
- 不把所有 touched files 的行数变少作为目标；目标是收敛关键 interface 和不变量。
- 不顺手修 workspace 既有 clippy debt，除非它直接阻塞该架构 seam 的编译或测试。
- 不改变 companion、hook、user mailbox 的产品语义；只让不同来源的 launch/wake eligibility 在 interface 中显式表达。

## Recommended MVP

第一轮实现只做三个深ening：

1. `StartupRecoveryPipeline`：收拢启动恢复 phase 顺序和报告。
2. `ControlEffectReplayPlan`：把 replay intent 和 batch drain 语义放进 AgentRun control-effect module。
3. `MailboxWakeAdmission`：集中 target delivery/runtime state decision，并让 companion/hook/user mailbox 通过同一个 decision surface。

`LaunchOrContinueTurn` 的拆分作为第二轮，除非第一轮 admission 抽取时发现必须同步调整。
