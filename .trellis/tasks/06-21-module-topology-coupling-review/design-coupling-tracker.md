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
| D07 | P0 | AgentFrame exposure model | visible canvas/module refs 是 frame revision、独立 exposure 表，还是 capability dimension | 需要决定后再创建实现任务；不要继续直接 UPDATE 当前 frame 扩张语义 | open |
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

1. Runtime coordinate design：D02、D03、D15。
2. Control surface design：D04、D08、D09、D18。
3. Capability surface design：D05、D06、D07、D13、D14。
4. Contract boundary design：D01。
5. Runtime failure projection design：D16、D17。

## Source Map

- Contract boundary: `research/10-contract-boundary-deep-dive.md`
- AgentRun control: `research/11-agentrun-control-deep-dive.md`
- Lifecycle runtime facts: `research/12-lifecycle-runtime-facts-deep-dive.md`
- Permission / frame / VFS / gateway: `research/13-permission-frame-vfs-gateway-deep-dive.md`
- Local placement / relay: `research/14-local-placement-relay-deep-dive.md`
