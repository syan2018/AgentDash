# Agent 启动路径主轴收束 Implement Plan

## Phase 0: Baseline Audit

- [x] 列出所有生产 agent materialization 和 turn launch 入口。
- [x] 标记入口类型：source adapter、materialization、mailbox delivery、turn launch、frame construction modifier。
- [x] 用 grep 固化当前重复路径清单，避免实施中遗漏。

Suggested commands:

```powershell
rg -n "LifecycleDispatchService::new|RuntimeSessionCreator|RuntimeSessionExecutionAnchor::new|AgentFrameBuilder::new|AgentFrameBuilder::new_launch_anchor|LaunchCommand::|launch_command" crates/agentdash-application/src crates/agentdash-api/src crates/agentdash-local/src
```

## Phase 1: Materialization Authority

- [x] 评估 `LifecycleDispatchService` 是否直接升级为唯一 materialization service，或抽出 `AgentMaterializationService`。
- [x] 将 workflow AgentCall node 从 `AgentNodeLauncher` 的自建流程迁到统一 materialization。
- [x] 删除 workflow node 内重复创建 `LifecycleAgent`、`RuntimeSession`、`AgentFrame`、anchor 的代码。
- [x] 保留 orchestration node policy 解析和 `NodeStarted` event 写入，但事件引用来自统一 materialization refs。

Validation:

```powershell
cargo test -p agentdash-application workflow::orchestration
cargo check -p agentdash-application
```

## Phase 2: Companion Dispatch Service

- [x] 从 `CompanionRequestTool::execute_sub_request` 抽出 companion child dispatch service。
- [x] tool 层只保留 payload / roster / hook gate / cancellation adapter。
- [x] dispatch service 统一创建 child agent/gate/lineage/runtime refs，并返回 launch modifier payload。
- [x] 保持 selected ProjectAgent binding 和 task assignment 语义，但避免散落在 tool 函数里。
- [x] 保持 wait / adoption mode 通过 durable `LifecycleGate` 表达。

Validation:

```powershell
cargo test -p agentdash-application companion
cargo check -p agentdash-application
```

## Phase 3: LaunchCommand Modifier Model

- [x] 设计 typed `LaunchModifier` / `LaunchSourcePayload`。
- [x] 将 `companion_hint`、`routine_hint`、`local_relay_mcp_servers`、`local_relay_workspace_root` 从 `LaunchCommand` 横向字段迁出。
- [x] 更新 source adapters：local relay、routine、hook auto-resume、AgentRun mailbox、companion dispatch。
- [x] 确保 `LaunchCommand` 的核心字段只表达通用 turn launch intent。

Validation:

```powershell
cargo test -p agentdash-application session::launch
cargo check -p agentdash-api
cargo check -p agentdash-local
```

## Phase 4: Frame Construction Pipeline

- [x] 将 `FrameConstructionService::classify` 改为 owner composer + modifier pipeline。
- [x] 保留 ProjectAgent / LifecycleNode / ExistingSurface 作为 owner surface composer。
- [x] 将 Companion 从最高优先级 route 改成 modifier。
- [x] 确保 pending runtime commands 仍在 close surface 阶段统一 replay。
- [x] 确保 VFS / MCP / capability closure 仍由 `FrameLaunchSurface` 校验。

Validation:

```powershell
cargo test -p agentdash-application agent_run::frame::construction
cargo test -p agentdash-application session::hub
```

## Phase 5: ProjectAgent Start And Mailbox Guard

- [x] 保持 ProjectAgent draft start 两阶段语义。
- [x] 如职责过厚，拆出 materializer / initial mailbox submitter / receipt coordinator，但不改变前端 contract，除非类型必须收束。
- [x] 检查 route handler 没有重新引入 launch / steer 判定。

Validation:

```powershell
cargo test -p agentdash-application agent_run
cargo check -p agentdash-api
pnpm --filter app-web test -- agentRunMailbox
```

## Phase 6: Spec And Cleanup

- [x] 更新 session startup / runtime state / execution frames 相关 spec。
- [x] 删除旧路径 wrappers 和 dead comments。
- [x] grep 确认没有非权威 production materialization。
- [x] 根据实际变更运行 focused checks，避免小规模迭代过度测试。

Final grep checks:

```powershell
rg -n "RuntimeSessionExecutionAnchor::new|RuntimeSessionExecutionAnchor::new_orchestration_dispatch|AgentFrameBuilder::new_launch_anchor|AgentFrameBuilder::new\\(" crates/agentdash-application/src
rg -n "companion_hint|routine_hint|local_relay_mcp_servers|local_relay_workspace_root" crates
```

## Rollback Points

- Phase 1 后单独提交或至少保留清晰 diff，因为 workflow AgentCall materialization 是最大结构变更。
- Phase 2 后单独验证 companion tests，避免 hook/gate/wait 语义混入 LaunchCommand 重构。
- Phase 3 / Phase 4 可以连续做，但必须在 modifier 类型稳定后再删除旧 fields。
