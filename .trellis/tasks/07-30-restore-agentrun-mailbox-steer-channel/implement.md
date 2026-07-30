# Implementation Plan

## Phase 0 — Baseline And Contract Lock

- [ ] 记录当前相关脏文件并保持完全不触碰。
- [ ] 定向读取 backend/frontend/cross-layer Trellis specs。
- [ ] 固化 Mailbox owner、envelope、explicit Submit/Steer、Hook composite outcome与 provenance contract。
- [ ] 将参考 worktree文件按 recover / rewrite / reject清单登记，不整目录覆盖。

Validation:

- PRD、design与spec术语一致。
- 负向源码清单可定位自动 Submit→Steer、同步 handoff、retired tables、Hook unsupported action和孤儿 Channel adapter。

## Phase 1 — Physical Recovery Before Wiring

- [ ] 按 `research/physical-recovery-manifest.md` 将 Domain、AgentRun Mailbox folder、Postgres repository、Rust contracts/API、frontend和wait source文件从 reference worktree完整复制到当前对应路径。
- [ ] 保持恢复文件不被 production `mod.rs`、bootstrap或route引用。
- [ ] 对每个恢复文件记录保留、删除和待适配依赖。
- [ ] 确认旧 migrations、generated TypeScript、`mailbox_runtime_adapter.rs`、`message_delivery.rs`、`control_effects.rs` 没有被复制进 production。

Validation:

- A 类文件与 reference逐文件可 diff。
- 恢复阶段不改变当前 binary编译图和运行行为。
- 没有 RuntimeSession adapter或旧 schema被意外重新声明。

## Phase 2 — Mailbox Domain And PostgreSQL

- [ ] 在 Phase 1恢复文件上收敛 Mailbox domain、commands、controls、payload、policy、receipts词汇。
- [ ] 在已恢复文件上移除 RuntimeSession字段，改为当前 Product/Complete Agent坐标。
- [ ] 新增 Mailbox messages/states/owner dispatcher lease/receipt schema和repository。
- [ ] 实现 source dedup、owner lease、message claim、reorder、pause/resume、settle、unknown recovery。
- [ ] 更新 migration retired/required table清单。

Validation:

- domain unit tests。
- embedded PostgreSQL顺序、lease、stale fencing、dedup、payload retention、recovery测试。
- clean migration与现有开发库顺序 migration。

## Phase 3 — Dash Owner Mutation Writer

- [ ] 在 `agentdash-agent` 内建立 source owner串行 mutation writer。
- [ ] 将 Core callbacks、Steer、terminal fence、surface/effect mutation迁入 writer。
- [ ] 保留 CAS external fencing语义。
- [ ] 增加 callback commit与Steer并发回归测试。

Validation:

- 定向 `agentdash-agent` / Native integration测试。
- 同一 owner并发 mutation全部成功且顺序确定。

## Phase 4 — AgentRun Module And Thin Complete Agent Adapter

- [ ] 恢复深 Mailbox intake/control/dispatch module。
- [ ] scheduler改用 Complete Agent `read/execute/inspect/live_batches`。
- [ ] `AgentRunProductCommand`新增显式 Steer，删除 Submit自动改写。
- [ ] 普通 runtime input route改为 Mailbox intake；即时控制仍直达。
- [ ] Agent command/history contract加入 origin/source/submission kind。
- [ ] Native、Codex、Remote adapter持久化并恢复 provenance。
- [ ] 确认 AgentRuntime crate没有 Mailbox、producer、Channel或Product Hook repository/policy。

Validation:

- idle Submit、active Queue、explicit Steer、non-steerable blocked。
- execute unknown / inspect applied-not-applied-unknown。
- restart/due scan与重复 live wake。
- 三种 Complete Agent adapter history projection。

## Phase 5 — AgentRun Product Hook And Boundary Consumption

- [ ] 在 AgentRun Product module恢复 canonical HookRun/HookEffect schema、repositories与workers。
- [ ] 保持旧 `agent_runtime_hook_plan/run/effect` retired。
- [ ] 用 composite Hook outcome替换单一 decision限制。
- [ ] Complete Agent surface支持 ContinueTurn/RefreshSurface。
- [ ] Native core接入 AfterTurn/BeforeStop safe-boundary callback。
- [ ] AfterTurn/BeforeStop message先materialize Mailbox，再claim并返回。
- [ ] 恢复 terminal HookEffect -> auto-resume Mailbox链。
- [ ] 恢复 Hook trace、effect retry、lease recovery与unknown inspection。

Validation:

- required AfterTurn/BeforeStop provisioning。
- composite action不丢语义。
- safe-boundary消费先有 durable message/claim。
- terminal auto-resume重复callback/restart只续跑一次。
- HookRun/HookEffect多worker与stale ack测试。

## Phase 6 — Channel, Companion And Other Producers

- [ ] 将 Channel adapter接入编译与production bootstrap。
- [ ] 建立 Channel plan -> admission -> Mailbox materialization -> state收敛链。
- [ ] Companion child/result/parent/human flows统一通过 Channel。
- [ ] Composer、Draft、Canvas、Routine、Workflow统一通过 Mailbox。
- [ ] 删除同步 Product input delivery和孤儿 Channel delivery代码。

Validation:

- 各 producer source identity/dedup测试。
- Companion parent active时默认 Pending，不产生硬 Steer。
- Channel跨 owner失败可重放。
- 负向源码检查无普通 input绕过 Mailbox。

## Phase 7 — API Contracts And Frontend

- [ ] 恢复 Mailbox API/view/update/control contracts并生成 TypeScript。
- [ ] 恢复 `MailboxMessageRow`、content projection与 Session integration。
- [ ] 恢复 Waiting / Steer / Pending和完整操作。
- [ ] 修正 Composer running状态按钮。
- [ ] Enter与Ctrl/Cmd+Enter形成不同 request。
- [ ] transcript按 authoritative provenance展示来源。

Validation:

- Rust contract tests与generation check。
- reducer/component/integration tests。
- 浏览器验证 Queue、Steer、Companion、Hook auto-resume与错误恢复。

## Phase 8 — Specs And Cross-layer Gate

- [ ] 更新 AgentRuntime Product/Kernel/Persistence/Native、Channel、Hooks、frontend和cross-layer specs。
- [ ] 删除同步 handoff、无 Mailbox、自动 Submit→Steer等冲突约定。
- [ ] 运行相关 crate定向 fmt/check/test与前端 lint/typecheck/test。
- [ ] 运行 migration guard、源码负向检查和关键 PostgreSQL集成测试。
- [ ] 使用 `trellis-check` 完成跨层复核。

## Risky Boundaries

- Dash repository writer切换期间不能与旧 mutation path并存。
- 物理恢复文件在适配完成前不得接入 production module，避免引入旧 RuntimeSession编译依赖。
- Mailbox schema与scheduler必须同一阶段启用，不能出现持久接收但无人恢复。
- Hook callback contract改变会影响 Host、Native、Codex、Remote和surface validation。
- Contract生成文件只能由生成流程更新。
- 共享脏工作区中的既有修改不属于本任务；任何重叠先停下协调。

## Rollback Model

项目未上线，不实现兼容回退。阶段内失败通过修正当前唯一模型继续推进；不得恢复同步 handoff、RuntimeSession adapter、dual-write或旧 endpoint。
