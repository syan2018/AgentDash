# 设计层面模块耦合追踪

## Purpose

本文件追踪 `research/followup-backlog.md` 中不适合直接机械执行的模块耦合问题。它们需要先讨论并确定事实源、控制面 owner、runtime 坐标语义或跨层 contract 形态，然后再拆成独立 Trellis task。

机械性重构项已移入子任务：

- `.trellis/tasks/06-21-architecture-review-mechanical-refactors/`

## Priority Board

| ID | Priority | Topic | Decision Needed | Current Recommendation | Status |
| --- | --- | --- | --- | --- | --- |
| D01 | P0 | application / contracts 边界 | application 是否允许构造 browser-facing contract DTO，还是 contract mapping 回到 API/application adapter 边界 | 先做 import-level 审计；长期应让 application read model 与 wire DTO owner 明确分层 | open |
| D02 | P0 | AgentRun delivery runtime resolver | run/agent/frame/node/attempt 的 delivery target selection policy 由谁拥有 | 建立 application-level selection service；repository raw latest 只做底层查询 | open |
| D03 | P0 | RuntimeSessionExecutionAnchor semantics | latest/primary/current-frame/run-scoped anchor 的语义如何统一 | 先定义 selection policy，再改 workspace/cancel/mailbox/SubjectExecutionView 消费 | open |
| D04 | P0 | Lifecycle start vs drain | public `start_lifecycle_run` 是否只创建 Ready run，drain 是否成为显式 command | 拆成 create Ready run 与 explicit drain/continue command | open |
| D05 | P0 | PermissionGrant runtime fact | grant status、RuntimeCapabilityTransition、AgentFrame capability 谁是运行态授权事实源 | AgentFrame/capability transition 应成为 runtime tool surface 的可恢复事实；grant status 负责审批/审计 | open |
| D06 | P0 | Canvas exposure fact | Canvas live VFS、AgentFrame visible refs、hook capability refresh 的恢复顺序 | 先确定 frame refs 或 capability transition 为可恢复事实源，再刷新 live VFS | open |
| D07 | P0 | AgentFrame exposure model | visible canvas/module refs 是 frame revision、独立 exposure 表，还是 capability dimension | 需要决定后再创建实现任务；runtime exposure 写入应绑定到可审计事实源 | open |
| D08 | P0 | Extension backend target | panel API、workspace module tool、RuntimeGateway 的 backend target resolver 如何统一 | 后端 resolver 统一 target；frontend 只表达 intent/context | open |
| D09 | P0 | Relay command target taxonomy | prompt/cancel/MCP/extension/terminal/VFS 分别绑定 execution placement、session route、mount utility 还是 setup | 先写命令分类 contract，再分批收敛调用点 | open |
| D10 | P1 | Command policy vs ConversationSnapshot | command availability 是否应从 UI snapshot 中抽出 core resolver | 抽出 command availability core，policy 与 snapshot 共用 | open |
| D11 | P1 | Status aggregation owner | orchestration status 与 run status 的 owner 边界如何写入 contract | application owns orchestration derivation，domain owns run aggregation | open |
| D12 | P1 | SubjectExecution history | SubjectExecutionView 是否需要表达 execution history，而非只给 latest node | 增加 history list，latest 从同一列表派生 | open |
| D13 | P1 | RuntimeGateway action/channel admission | extension channel 是否需要云端 method permission known-key 预检 | action/channel 入站 admission 对齐，local host 保留二次裁决 | open |
| D14 | P1 | WorkspaceModule visibility | CapabilityState workspace_module 与 AgentFrame runtime refs 的审计事实源 | 建立统一 visibility resolver；明确 runtime refs 是否进入 capability state | open |
| D15 | P1 | AgentRun resource surface coordinate | current frame VFS 与 anchor launch frame address 如何共存 | DTO 必须表达 surface source 坐标；选择策略应复用 anchor selection 决策 | open |
| D16 | P1 | Backend disconnect terminal projection | backend disconnect 如何转成用户可见 lost/terminal projection | 需要先验证当前 stream/feed 行为，再定 projection owner | open |
| D17 | P1 | MCP backend fallback | session context 下 MCP 是否允许 VFS/catalog/any backend fallback | 推荐 session context 强制 session route/backend execution；setup/probe 才允许 fallback | open |
| D18 | P1 | Terminal vs execution lease | terminal 是 mount utility 还是 session execution surface | 先定产品语义，再决定是否引入 lease/active-session 投影 | open |

## Discussion Clusters

| Cluster | Items | Owner Modules | Discussion Focus | Task Creation Direction |
| --- | --- | --- | --- | --- |
| Runtime Coordinate | D02, D03, D12, D15 | AgentRun workspace, RuntimeSessionExecutionAnchor, SubjectExecutionView, VFS surface | 统一 run / agent / frame / node / attempt 到 delivery runtime 的选择策略，并让 resource surface 坐标与 anchor selection 共享同一套语义。 | 先创建 design task 定义 selection policy，再拆 workspace、cancel、mailbox、SubjectExecutionView 消费面实现任务。 |
| Control Surface | D04, D08, D09, D10, D18 | Lifecycle command, ConversationSnapshot, Extension RuntimeGateway, Relay, Terminal | 区分 execution-placement-bound、session-route-bound、mount-utility-bound 与 setup-bound command，明确 UI snapshot 与 command policy 的共享 resolver。 | 先产出 command taxonomy / lifecycle command contract，再分批收敛 lifecycle start-drain、extension target、terminal 语义。 |
| Capability / Exposure Fact | D05, D06, D07, D13, D14 | PermissionGrant, AgentFrame, Canvas expose, WorkspaceModule visibility, RuntimeGateway admission | 确定 grant status、frame revision、capability transition、workspace module refs 中哪一个承载运行态可见能力事实，并定义恢复顺序。 | 先做事实源设计任务，再拆 PermissionGrant effect、Canvas expose recovery、WorkspaceModule visibility resolver、channel admission parity。 |
| Contract Boundary | D01 | application, contracts, API adapter, frontend generated contracts | 确定 application read model 与 browser-facing wire DTO 的 owner，梳理 `agentdash-contracts` 内部 conversion 的允许边界。 | 先做 import-level audit task，输出 application read model / API adapter / contract DTO owner map，再按 owner 迁移高风险入口。 |
| Runtime Failure / Placement | D16, D17 | Relay, BackendRegistry, MCP relay, local backend, session route | 将 backend disconnect、MCP backend fallback、local backend identity 投影到用户可见状态和执行目标选择。 | 先做 characterization task 验证当前 stream/feed/route 行为，再创建 projection 或 fallback 收敛任务。 |

## Follow-up Parent Tasks

| Cluster | Trellis Task | Status |
| --- | --- | --- |
| Runtime Coordinate | `.trellis/tasks/06-21-runtime-coordinate-convergence/` | planning |
| Control Surface | `.trellis/tasks/06-21-control-surface-command-boundary/` | planning |
| Capability / Exposure Fact | `.trellis/tasks/06-21-capability-exposure-fact-convergence/` | planning |
| Contract Boundary | `.trellis/tasks/06-21-contract-boundary-ownership-audit/` | planning |
| Runtime Failure / Placement | `.trellis/tasks/06-21-runtime-failure-placement-convergence/` | planning |

## Direct Refactor Candidates

这些候选项可以先作为较小 Trellis task 推进，原因是它们有明确行为边界或验证入口，不依赖完整簇级设计定稿。

| Candidate | Related Items | Module Scope | Acceptance Direction |
| --- | --- | --- | --- |
| Hook mailbox NotFound fallback 收口 | D02, D03 | `session/mailbox_delegate.rs`, AgentRun mailbox, Agent loop turn boundary | anchored AgentRun mailbox missing 进入 diagnostic/error；unbound trace 继续通过 direct path 表达。 |
| Task execution surface 收敛 | D12 | SubjectExecutionView, TaskExecutionView, `task_read` tool | public execution projection 从 SubjectExecutionView 读取；narrow TaskExecutionView service 或 execution mode 有明确私有/移除结论。 |
| Backend disconnect terminal projection 验证 | D16 | Relay registry, lease repo, session stream, frontend feed | 用测试或 trace 验证 disconnect 后 running prompt 是否产生 lost/terminal projection，并记录 projection owner。 |
| Extension channel admission parity | D13 | RuntimeGateway, extension channel, local host bridge | channel method permission known-key 预检与 action admission 对齐，local host 继续执行运行时二次裁决。 |
| Standalone local backend id 来源收口 | Runtime Failure / Placement | `agentdash-local` CLI, desktop ensure, dev runtime | standalone identity 来源被明确为 claim/ensure 或 debug/internal path，runtime-summary 与配置文案一致。 |

## Decision Notes Template

每个设计项进入讨论时，在对应条目下追加：

```md
### Dxx Decision Notes

- Decision:
- Why:
- Owner modules:
- Rejected alternatives:
- Follow-up Trellis task:
- Acceptance direction:
```

## Initial Task Split Recommendation

建议后续设计讨论按以下顺序拆：

1. Runtime Coordinate：D02、D03、D12、D15。
2. Control Surface：D04、D08、D09、D10、D18。
3. Capability / Exposure Fact：D05、D06、D07、D13、D14。
4. Contract Boundary：D01。
5. Runtime Failure / Placement：D16、D17。

小重构候选可以穿插执行，执行前应声明它依赖的簇级 owner 决策边界。

## Decision Notes

### D02 / D03 Decision Notes

- Decision: AgentRun delivery runtime selection 必须全系统统一；AgentRun 应持有或可唯一解析 current delivery binding。
- Why: workspace、cancel、mailbox、SubjectExecutionView 和 resource surface 各自查询并解释 latest anchor，会在多 run、多 frame、retry、append orchestration、replacement session 场景中选择不同执行目标。
- Owner modules: `agentdash-application::agent_run`, `agentdash-application::lifecycle`, RuntimeSessionExecutionAnchor repository, AgentRun workspace, mailbox, subject execution control。
- Rejected alternatives: 让各消费方继续按自己的局部上下文查询 anchors；让 repository `latest` 承担业务 selection 语义。
- Follow-up Trellis task: `.trellis/tasks/06-21-runtime-coordinate-convergence/`
- Acceptance direction: workspace、cancel、mailbox、SubjectExecutionView 通过统一 delivery binding / selection service 消费当前执行目标；repository raw latest API 降级为底层排序查询。

### D05 / D06 / D07 Decision Notes

- Decision: AgentFrame 是 runtime capability / exposure 的唯一锚定事实源。
- Why: PermissionGrant status 负责审批和审计；live VFS、WorkspaceModule visibility、hook runtime refresh 都是运行态派生面。如果这些面并列承载事实，失败恢复会产生中间态。
- Owner modules: AgentFrame domain/repository, permission service, canvas expose tools, WorkspaceModule visibility, session capability service, RuntimeGateway admission。
- Rejected alternatives: PermissionGrant status 直接作为 runtime capability；Canvas expose 先更新 live VFS 再补 frame refs；WorkspaceModule runtime refs 分散在 CapabilityState 与 AgentFrame JSON 中。
- Follow-up Trellis task: `.trellis/tasks/06-21-capability-exposure-fact-convergence/`
- Acceptance direction: approve/revoke/expire、Canvas expose、WorkspaceModule visibility 都写入或读取 AgentFrame capability/exposure fact；live VFS 与 hook runtime surface 从该 fact 派生并可恢复。

## Source Map

- Contract boundary: `research/10-contract-boundary-deep-dive.md`
- AgentRun control: `research/11-agentrun-control-deep-dive.md`
- Lifecycle runtime facts: `research/12-lifecycle-runtime-facts-deep-dive.md`
- Permission / frame / VFS / gateway: `research/13-permission-frame-vfs-gateway-deep-dive.md`
- Local placement / relay: `research/14-local-placement-relay-deep-dive.md`
