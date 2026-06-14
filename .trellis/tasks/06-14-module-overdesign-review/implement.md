# 执行计划

## 并行审查

- [x] 分派 Lifecycle / Workflow / Task 深度 review。
- [x] 分派 AgentRun / Session / Runtime Gateway 深度 review。
- [x] 分派 VFS / Local / Relay / Extension review。
- [x] 分派 Frontend / Contracts / Permission review。

## 主会话扫描

- [x] 统计模块规模、热点文件、跨层引用和重复状态/DTO 命名。
- [x] 抽样阅读 `Lifecycle`、`AgentRun`、session feed、runtime gateway、workflow/task projection 的核心文件。
- [x] 对 subagent 发现做去重和证据复核。

## 汇总

- [x] 写入 `overdesign-review.md`，按优先级和模块面组织。
- [x] 标记适合后续拆任务的清理候选。
- [x] 确认本轮没有修改业务代码。

## 验证

- `git status --short`
- 人工复核报告中的文件路径和关键证据是否真实存在。

## 第一轮并行收束

- [x] Lifecycle runtime truth source
  - [x] cancel 通过 `OrchestrationRuntimeEvent::NodeCancelled` / reducer 路径 materialize。
  - [x] Task projection 从 `SubjectRef(Task)` / association / anchor / runtime node 坐标派生。
  - [x] 删除 Running task absence -> Failed fallback。
  - [x] LifecycleRun status aggregation 保留单一 owner，并补 focused tests。

- [x] AgentRun control surface
  - [x] 收敛 workspace projection / conversation snapshot / command policy 的 command availability 计算。
  - [x] RuntimeSession runtime-control 收窄为 trace/detail/anchor backlink，不再复制 mailbox/action。
  - [x] 前端消费路径移除重复 action/mailbox 来源。
  - [x] 保留 mailbox durable intake/scheduler，不做大规模 delegate trait 拆分。

- [x] Permission / contract capability surface
  - [x] `/permission-grants` 支持 pending/active/terminal status query，而不是 active-only 后过滤。
  - [x] `ScopeEscalationIntent`、`PolicyDecision`、`PolicyOutcome` 进入 generated contract。
  - [x] companion capability grant 不再作为授权结果事实源。
  - [x] capability/tool catalog 经 `agentdash-contracts` 投影，前端 editor 不再镜像后端 visibility baseline。

## 第一轮验证

- [x] 后端 targeted tests / compile check 覆盖 Lifecycle、AgentRun、Permission 改动。
- [x] 前端 typecheck 覆盖 generated contract 与 UI 消费改动。
- [x] `git status --short` 确认只包含本任务相关变更。
- [x] 将未处理的 VFS / Local / Extension 装配层瘦身记录为后续候选，不混入第一轮实现。

## 残余风险

- 未跑全量 backend clippy、workspace tests、frontend tests、e2e；本轮使用 targeted backend/frontend 检查覆盖三条主线。
- `/tasks/{id}/execution` 仍是 route-local 轻量 DTO，尚未完全收敛到 generated `SubjectExecutionView`。
- companion payload registry 仍保留 `capability_grant_result` 类型；session UI 已不再提交该授权结果，platform broker 当前仍拒绝未闭环入口。

## 第二轮并行收束

- [x] Runtime tool composer
  - [x] 拆出 session-level composite tool provider / composer。
  - [x] VFS provider 只负责 `VfsToolFactory` / VFS read-write-execute cluster。
  - [x] Workflow、collaboration、workspace module / extension runtime 工具各自进入窄 provider。
  - [x] 保持现有工具 surface 行为不变，并补 focused tests 或 compile checks。

- [x] Local command router
  - [x] 保留 `RelayMessage` 顶层 wire enum。
  - [x] `CommandHandler` 瘦身为 `LocalCommandRouter` 或等价薄分发层。
  - [x] tool / extension / terminal / materialization / MCP / prompt 等 domain handlers 只接收各自依赖。
  - [x] 运行 `agentdash-local` targeted tests / cargo check。

- [x] Extension / VFS surface contract
  - [x] Extension Host 不再接受 raw `workspace_root` 覆盖 session workspace context。
  - [x] process/env permission 语义收窄，manifest/SDK/Rust guard 保持一致。
  - [x] RuntimeGateway/local host 对 extension action/channel input/output schema 有明确校验 owner。
  - [x] VFS browser 与 extension webview mount/backend selection 使用共享策略或后端 usage hint。

## 第二轮验证

- [x] Rust targeted tests / cargo check 覆盖 application/local/api/domain 改动。
- [x] Frontend typecheck / targeted vitest 覆盖 selector/contract 改动。
- [x] 本轮未修改 generated contract，未运行 `pnpm run contracts:check`。
- [x] 记录未处理的 `vfs/mount.rs` 全量拆分与 Tauri profile/claim 后续候选。

## 第二轮残余风险

- `vfs/mount.rs` 仍可继续按 provider / metadata / validation / operations 拆分；本轮只收束 session runtime tool 装配边界。
- Tauri profile/claim 仍可继续下沉到 local runtime library；本轮只处理 relay router 与 Extension Host contract。
- JSON Schema validation 覆盖当前插件协议子集：`true/false` schema、`type`、`required`、`properties`、`additionalProperties: false`、`items`、`enum`、`const`。
