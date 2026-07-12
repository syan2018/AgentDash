# Local Runtime无数据库Host状态重建实施计划

## Current Baseline

已完成但尚未提交：`HostIncarnationId`端到端传递、旧incarnation拒绝、inventory identity换代、ephemeral Host repository、Local PostgreSQL移除和基础diagnostics。以下阶段先补canonical recovery，再做真实产品验证。

## Phase 1: Freeze Contracts And Schema

- [ ] 更新Runtime Kernel、AgentRun facade、Driver Host、Runtime Wire specs，明确same-thread rebind、binding epochs与declared-loss后route终态。
- [ ] 定义`BindingEpoch`、recovery intent DTO/state、`ThreadRebind` command与`BindingReestablished` event。
- [ ] 追加`0068_agent_runtime_binding_recovery.sql`：product thread anchor、append-only binding lineage、recovery intent、Host同thread历史binding/source constraints与非终态partial unique。
- [ ] 增加migration/schema guard；禁止修改0061/0064/0065/0067。

Gate：schema必须能表达“稳定thread + 多个binding epoch + 一个非终态binding”，否则不进入恢复编排。

## Phase 2: Host And Runtime Rebind Invariants

- [ ] Host repository允许同thread的Lost历史binding与单一新active binding；memory/Postgres conformance一致。
- [ ] `Resume`严格校验Driver返回old canonical source thread；新offer必须保证Resume并满足旧surface/hook。
- [ ] Managed Runtime实现`ThreadRebind` admission/projection/journal，保留context、transcript与sequence。
- [ ] late old binding/generation event quarantine；恢复前guard因revision推进而stale。
- [ ] outbox worker按operation terminal/current generation收口旧work。

Gate：kernel/Host测试证明同一thread cursor连续、旧binding不可dispatch、新binding可dispatch。

## Phase 3: Product Binding Lineage And Recovery Coordinator

- [ ] 将单行`agent_run_runtime_binding`repository拆为stable anchor + lineage；初始epoch 1保持ThreadStart-before-thread语义。
- [ ] current binding在Thread存在后从Runtime projection join lineage解析；`list_by_run/list_by_agent`保持每target一个current结果。
- [ ] 实现durable recovery intent repository、active-intent唯一约束与四个崩溃点reconciliation。
- [ ] Provisioner增加`recover`：固定原definition/placement owner，选择新offer，复制旧surface，mark old Lost，Host Resume bind，append lineage，Runtime ThreadRebind。
- [ ] `send_message`/mailbox drain在Lost时按需recover后创建新Turn；inspect只报告状态，不触发side effect。
- [ ] 移除Backend注册时旧Runtime Wire route reopen；新binding解析新placement。

Gate：E2E证明`BindingLost -> new inventory -> binding epoch+1 -> BindingReestablished -> new Turn terminal`，重复/并发恢复不产生第二binding。

## Phase 4: Local No-Database Completion

- [x] production `EphemeralAgentRuntimeHostRepository`与conformance。
- [x] Local bootstrap新incarnation、definitions/instances/offers重建。
- [x] `agentdash-local`移除PostgresRuntime、Dashboard migration、pool/runtime handle与依赖。
- [x] inventory/proxy identity纳入incarnation，Local/Cloud基础diagnostics。
- [ ] AgentRun recovery diagnostics/inspect状态补齐。
- [ ] 验证旧Local DB目录存在或缺失均完全不读取。

## Phase 5: Directed Verification

- [ ] Runtime kernel：Lost-only rebind、stale old coordinates、profile/surface rejection、context/transcript/cursor连续、late event quarantine。
- [ ] Host conformance：同thread多epoch、单active约束、Resume source exactness、old lease/dispatch rejection。
- [ ] Product repository：bootstrap current、post-rebind current join、lineage审计、run/agent list语义。
- [ ] Recovery：prepared/host_bound/lineage/runtime committed崩溃恢复、并发CAS、inventory-not-ready、unsupported Resume。
- [ ] Relay：declared loss不reopen旧route，新binding使用新stream identity。
- [ ] Mailbox/outbox：accepted Lost不漂移、queued恢复后dispatch、旧generation work终止。
- [ ] 相关crate `cargo check --tests`、定向test、fmt与diff check；闭环前不跑全仓。

## Phase 6: Real Product Verification

- [ ] `pnpm dev`：首次启动无Local Postgres，Backend online，首轮AgentRun成功。
- [ ] 杀Local Runtime并以新incarnation重启：旧run Lost；下一条消息经mailbox恢复到同thread的新binding epoch并成功完成。
- [ ] 从恢复前cursor读取完整`BindingLost -> BindingReestablished -> new Turn`事件序列。
- [ ] `pnpm dev:desktop`验证external cloud + embedded runner无数据库启动与恢复。
- [ ] Standalone Runner前台/service验证无数据库、断线重连、进程重启恢复。
- [ ] 观察进程树与data root，确认无postgres进程、新数据目录或`_sqlx_migrations`。

## Review Gates

- 不创建replacement RuntimeThread，不引入复合AgentRun event cursor。
- 不原地复活旧Host binding，不reopen declared-lost placement。
- 不跨Backend failover，不把Resume失败降级为Start。
- 不让Local Runtime重新依赖任何数据库。
- 任何已应用migration只读；所有schema变化追加新migration。

## Validation Record (2026-07-12)

- 真实Dashboard migration从schema 67升级到68成功；Local启动路径不再构造PostgreSQL或执行Dashboard migrations。
- `pnpm dev:web:skip-build -- --workspace-roots D:/Projects/AgentDash`完成Cloud、Local与Web启动；首次AgentRun收到预期响应。该UI样本绑定Cloud native-agent，因此不作为Local Runtime Wire断连恢复证据。
- 定向质量门通过：Host binding loss/lease fence、remote Runtime Wire exactly-once disconnect、Cloud route disconnect acknowledgment、recovery intent epoch/order，以及相关crate check/fmt/diff check。
- enterprise remote-runtime E2E明确使用Runtime Wire，但当前在首次tool callback后的continuation阶段超时，未进入该测试的disconnect段；事件停在tool result后的下一条ItemStarted。该既存tool-continuation回归归入父任务`07-11-agent-runtime-debug-regression-tracking`，不改变本任务的Local无数据库与按需rebind契约。
