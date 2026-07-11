# PR #93 与当前分支冲突地图

> 调查日期：2026-07-11
> 调查方式：本地 `gh`、远端 ref fetch、`git merge-base`、`git log --left-right`、
> `git diff --name-status/--numstat` 与不触碰 index/worktree 的 `git merge-tree`。
> 本文只评估整合方案；未执行 merge、rebase、cherry-pick 或产品代码修改。

## 1. 结论

唯一目标是 [PR #93](https://github.com/syan2018/AgentDash/pull/93)
`feat(agent-runtime): 完成可插拔 Agent Runtime 架构收敛`。PR 当前为 Open、非 Draft，GitHub
报告其相对 `main` 为 `MERGEABLE/CLEAN`；这不代表它能与当前工作分支直接干净合并。

不建议把 PR 直接 merge 到当前分支，也不建议将当前 73 个提交逐个 rebase/cherry-pick 到 PR。
两侧从同一基点出发，同时删除旧 RuntimeSession/Canvas/action gateway，但分别建立了 Managed
Runtime 与 actor-neutral Operation/Interaction/Channel 新边界。文件级合并会产生 55 个显式冲突，
并让更多自动合并文件留下双事实源。

推荐以 PR head `efdfa5dc` 为新整合基线，按当前分支的领域主题重放最终语义：保留 PR 的
Managed Runtime、RuntimeThread/Binding、Driver Host、Tool Broker 与 AgentRun facade；保留当前分支的
canonical application Operation、OperationScript、exact MCP/Extension provider、Workspace Interaction、
Channel V2 与 Extension Component。两套 `Operation`/`Interaction` 必须明确分层，不合并成同一状态机：

- Managed Runtime `Operation/Interaction` 持有 Agent Thread 的命令、审批、输入与 durable execution 状态；
- application `OperationRef/OperationExecutionCore` 持有 MCP、Extension、Interaction、Workflow 等业务能力
  的 exact provider identity、授权与调用语义；
- Runtime Tool Broker 持有 Item/ToolCall 的接受、恢复与 terminal evidence，并通过受信 executor adapter
  调用 application Operation；不能再建立第二套 provider admission，也不能恢复 Session-bound gateway。

## 2. 固定提交坐标

| 坐标 | SHA / 值 |
| --- | --- |
| 当前分支 | `codex/workspace-duplex-interaction-planning` |
| 当前 HEAD / 远端存档点 | `7070f6b0c28963c4cd04c67312ebd0571189e4b4` |
| PR base branch | `main` |
| PR base SHA | `957fa9d60ea3d67efa1bb278fe5b376cf0c34598` |
| PR head branch | `codex/agent-runtime-architecture-convergence` |
| PR head SHA | `efdfa5dc585530b1c8285e9b2a399ba92830c45e` |
| 当前 HEAD 与 PR head merge-base | `957fa9d60ea3d67efa1bb278fe5b376cf0c34598` |
| `merge-tree` synthetic result tree | `796fa9d2a96eaf62cfffa57bf79f97f986966a8a`（含未解决项，不是可提交结果） |

PR 的关键提交顺序为：

1. `22441a50`、`1330a856`：架构规划与目标 crate 拓扑；
2. `b43d2be5`：Runtime Contract/Wire；
3. `63dbd623`：Managed Runtime 状态内核；
4. `0806457d`：可恢复 context compaction；
5. `ef4bdec6`：PostgreSQL RuntimeCommit 与恢复队列；
6. `b47164bc`：Hook、Business Surface、Tool Broker、Driver Host；
7. `e934c287`：Native/Codex/Remote Runtime adapter；
8. `af21f9d7`：AgentRun/API/UI 生产切换、0065 cutover 与旧架构删除；
9. `fba13fbc`、`efdfa5dc`：任务归档与 journal。

## 3. 三方差异统计

`git rev-list --left-right --count 7070f6b0...efdfa5dc` 为 `73 11`；
`--cherry-pick` 结果仍为 `73 11`，没有可由 patch-equivalence 自动消去的整提交。

| 比较 | 文件 | 状态 | insertions | deletions |
| --- | ---: | --- | ---: | ---: |
| merge-base → 当前 HEAD | 356 | A 90 / M 212 / D 54 | 28,781 | 32,610 |
| merge-base → PR head | 626 | A 162 / M 275 / D 177 / R 12 | 70,765 | 106,711 |
| 当前 HEAD → PR head | 885 | A 216 / M 390 / D 267 / R 12 | 101,916 | 134,033 |

两侧精确路径交集为 95 个；当前侧独有 261 个，PR 侧独有 531 个。95 个交集里，
`merge-tree` 报告 55 个显式冲突：31 个 content conflict、24 个 modify/delete conflict。
剩余 40 个自动合并交集仍需语义审查，自动合并不等于架构正确。

## 4. `merge-tree` 显式冲突分类

### 4.1 Specs 与 workspace 记录：12 个 content

- Backend/spec：`.trellis/spec/backend/architecture.md`、`capability/architecture.md`、`index.md`、
  `runtime-gateway.md`、`session/architecture.md`、`vfs/vfs-access.md`、`workflow/architecture.md`；
- Cross-layer/frontend spec：`.trellis/spec/cross-layer/frontend-backend-contracts.md`、
  `.trellis/spec/frontend/architecture.md`、`.trellis/spec/frontend/state-management.md`；
- Append-only workspace：`.trellis/workspace/codex-agent/index.md`、`journal-2.md`。

这些不能选 ours/theirs。最终 spec 应同时描述 AgentRunRuntime facade 和 application Operation seam，
并删除旧 RuntimeSession 事实源；journal 仅按时间合并双方条目。

### 4.2 API：6 个 content + 3 个 modify/delete

Content：

- `crates/agentdash-api/src/agent_run_runtime_surface.rs`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/bootstrap/repositories.rs`
- `crates/agentdash-api/src/bootstrap/vfs.rs`
- `crates/agentdash-api/src/dto/mod.rs`
- `crates/agentdash-api/src/integrations.rs`

Modify/delete：

- PR 删除 `agent_run_terminal_control.rs`、`bootstrap/session.rs`，当前分支修改；
- 当前分支删除 `routes/canvases.rs`，PR 修改。

结论：以 PR 的 Agent Runtime composition 与删除为准；把 Operation、Interaction、Channel 的 repository、
route 与 provider composition 重新装配进 PR AppState。Canvas route 由 Interaction API 取代，
不移植 PR 对旧 `canvases.rs` 的适配。

### 4.3 AgentRun：2 个 content + 5 个 modify/delete

Content：`agent_run/mod.rs`、`runtime_surface_update.rs`。

PR 删除、当前分支修改：旧 `mailbox/delivery.rs`、`mailbox/mod.rs`、`mailbox/scheduler.rs`、
`mailbox_runtime_adapter.rs`、`runtime_surface.rs`。

结论：PR 的 `runtime_facade.rs`、`runtime_mailbox.rs`、`business_frame_surface_query.rs` 和
`AgentRunRuntimeBinding` 是目标入口。当前分支在旧 mailbox/surface 中新增的逻辑不能保留文件，
只提取以下业务不变量重做：

- AgentFrame 不再持有独立 visible Canvas mount，WorkspaceModule/Interaction ref 是业务 surface；
- exact MCP/Extension/Interaction Operation 从当前 AgentFrame 与 attachment/capability 投影；
- server-owned execution identity 改用 `RuntimeThreadId + AgentRunRuntimeBinding`，删除当前分支新增的
  SPI `AgentRunExecutionRef`，不提供双路径。

### 4.4 Runtime Gateway / RuntimeSession：5 个 modify/delete

- 当前分支删除 `runtime_gateway/extension_actions.rs`，PR 只修改该旧实现；保持删除；
- PR 删除 RuntimeSession crate 内 `launch/plan.rs`、`tool_assembly.rs`、`turn_processor.rs`、
  `turn_supervisor.rs`，当前分支修改；保持 PR 删除并丢弃 fixture-only 变更。

PR 同时从 workspace 删除整个 `agentdash-application-runtime-session` crate 和数据库 RuntimeSession 表。
当前分支“RuntimeSession 仅保留 connector delivery/trace”的中间结论已被 PR 的 RuntimeThread/Driver
模型覆盖，不应保留兼容 crate、字段或 fallback。

### 4.5 Application composition：5 个 content + 4 个 modify/delete

Content：`application/src/channel.rs`、`companion/tools.rs`、`frame_construction/mod.rs`、
`frame_construction/owner_bootstrap.rs`、`lib.rs`。

Modify/delete：当前分支删除 `canvas/diagnostics.rs`；PR 删除 `relay_connector.rs`、
`task/context_builder.rs`、`vfs_owner_providers.rs`。

结论：Canvas diagnostics 保持删除；PR 的 Relay/Context/VFS 旧入口删除保持。Channel V2 的
owner-local key、binding index、provider/service admission 保留，但它向 AgentRun 投递时必须调用
PR 的 runtime mailbox/facade，不再构造已由 0065 删除的 delivery session/turn/receipt 字段。

### 4.6 Workspace Module：1 个 content + 6 个 modify/delete

Content：`workspace_module/mod.rs`。

当前分支删除、PR 修改：`runtime_bridge.rs`、`runtime_context.rs`、`runtime_tool_provider.rs`、
`surface.rs`、`tools.rs`、`visibility.rs`。

结论：保持当前分支对旧 Canvas WorkspaceModule runtime 的物理删除。PR 在这些文件中完成的
`delivery_runtime_session_id → runtime_thread_id` 和 execution-anchor → runtime-binding 迁移，不能通过
恢复文件保留；应移植到当前分支的新 `crates/agentdash-workspace-module/src/runtime_tool_provider.rs`
及 Operation/Interaction provider adapter。

### 4.7 其他：5 个 content + 1 个 modify/delete

- Content：`agentdash-application-workflow/Cargo.toml`、AgentRun lineage PostgreSQL repository、
  `agentdash-integration-api/src/lib.rs`、`agentdash-spi/src/lib.rs`、`agentdash-test-support/src/workflow.rs`；
- PR 删除 `agentdash-executor/src/connectors/pi_agent/connector_tests.rs`，当前分支只补 fixture；保持删除，
  验证迁移后的 Native integration tests 覆盖同一行为。

## 5. 未形成文本冲突但必须语义处理的热点

### 5.1 Cargo 与 crate 拓扑

当前分支未修改根 `Cargo.toml`；PR 在根 workspace 增加：

- `agentdash-agent-runtime-contract`
- `agentdash-agent-runtime`
- `agentdash-agent-runtime-host`
- `agentdash-agent-runtime-wire`
- `agentdash-agent-runtime-test-support`
- `agentdash-integration-native-agent`
- `agentdash-integration-codex`
- `agentdash-integration-remote-runtime`
- `agentdash-llm-provider`

并删除 `agentdash-application-runtime-session`。根 manifest 应采用 PR；当前分支对 ports、runtime-gateway、
workflow、infrastructure 的依赖按最终代码补入。`Cargo.lock` 虽可自动合并，也必须在最终 manifests
完成后重新生成，不能把 merge 结果当作有效 lock。

### 5.2 Migration 0061–0065

两侧文件名不同，所以 Git 没有报告 add/add conflict，但 SQLx migration version 已冲突：

| 当前分支 | PR |
| --- | --- |
| `0061_reset_channel_registry_v2.sql` | `0061_agent_runtime_managed_state.sql` |
| `0062_interaction_canvas_replacement.sql` | `0062_agent_runtime_hook_orchestration.sql` |
| — | `0063_agent_runtime_tool_broker.sql` |
| — | `0064_agent_runtime_driver_host.sql` |
| — | `0065_agent_runtime_cutover.sql` |

PR 的 0061–0065 是一个内部有 FK/删除依赖的连续 cutover，应原号保留。以 PR 为基线后，将当前分支
Channel 与 Interaction migrations 重编号为 0066、0067。两者分别修改 `lifecycle_runs.channel_registry`
及新增 `interaction_*`/删除旧 Canvas 表，与 PR 0065 的 RuntimeSession 表删除不存在直接 SQL 依赖，
适合在 0065 后执行。项目未上线，不需要历史编号兼容或双 migration 路径。

### 5.3 Contracts 与 generated TypeScript

精确路径交集只有 `crates/agentdash-contracts/src/generate_ts.rs` 与
`project/contract.rs`；后者两侧 patch 相同，前者分别注册 Runtime 与 Interaction/Extension contracts。
两侧生成的 TS 文件大多互不重名，因此不会产生文本冲突，但必须从最终 Rust source 重新生成：

- 保留 PR 的 `agent-runtime-contracts.ts`、`agent-runtime-wire.ts` 与 schemas；
- 保留当前分支的 `interaction-contracts.ts`、Extension/WorkspaceModule 最终合同；
- 删除 `canvas-contracts.ts`；
- 不手工拼接 generated TS，以 `pnpm contracts:generate` 的结果为准。

### 5.4 MCP、Extension 与 Tool Broker

PR Tool Broker 是 Agent Runtime Item/ToolCall 的 durable execution authority；当前分支 Operation core
是业务 capability 的 discover/admit/invoke authority。两者应串联，而非二选一：

```text
Runtime Item / ToolCall
  -> Platform Tool Broker (accept/recovery/approval/terminal)
  -> trusted Operation executor adapter
  -> OperationExecutionCore (principal/scope/exact provider/re-admission)
  -> MCP / Extension / Interaction provider
```

当前 `CurrentSurfaceRuntimeMcpAccess` 仍把 RuntimeThread 填入名为 `runtime_session_id` 的 surface/call
字段。整合时应直接改成 RuntimeThread/Binding 词汇并使用 PR 的 `BusinessFrameSurfaceQuery`，不保留
字段别名。旧 `extension_actions.rs` 不恢复；Extension exact Operation provider 和 Component host 保留。

### 5.5 两种 Interaction

PR `agent_runtime_interaction` 表示审批、user input、MCP elicitation 等 Agent Thread pending interaction；
当前分支 `interaction_definition/instance/event/...` 表示 Canvas/Workspace 的持久协作界面。二者生命周期、
owner 与状态机不同，应保持 crate/table/API namespace 分离。Workspace Interaction effect 可通过 application
Operation 调用业务能力；只有需要 Agent 响应的 runtime request 才进入 Managed Runtime Interaction。

### 5.6 Frontend

两侧 `packages/app-web` 各改 44/54 个文件，精确重叠仅
`AgentRunWorkspacePage.workspace-module.test.ts`，且修改不同 hunk。文本合并风险低，但 PR 已重写
AgentRun runtime feed、command availability 与 session projection，当前分支又重写 Canvas/Interaction、
Extension 与 Workspace panel。应以 PR AgentRun page/feed 为基线，再接入当前 Interaction/Extension UI，
最后通过重新生成的 contract 做 typecheck；不能因为无文本冲突就直接接受组合结果。

## 6. 机械移植与语义重做边界

### 可机械处理

- PR 新 runtime/integration crates、root workspace member 与独立 schemas；
- 两侧互不重叠的任务 archive、测试和大多数新增源文件；
- `project/contract.rs` 相同的 Box patch；
- PR 已删除而当前只改 fixture 的 RuntimeSession/old connector 文件：直接采用删除；
- 当前已删除而 PR 只改旧入口的 Canvas route、`extension_actions.rs`：直接采用删除；
- 最终 source 收敛后重新生成 Cargo.lock 与 TS/JSON contracts。

### 必须按最终模型重做

- AgentRun facade/mailbox/surface 与 API/bootstrap composition；
- `AgentRunExecutionRef` 到 `RuntimeThreadId/AgentRunRuntimeBinding` 的 identity 迁移；
- Tool Broker 到 application Operation 的受信执行 adapter；
- MCP surface/query/call context 的 RuntimeThread 适配；
- WorkspaceModule 新 runtime tool provider 对 PR Business Surface 的 contribution；
- Channel delivery 到 PR runtime mailbox；
- migration 重编号与 0065 后的 clean cutover；
- Runtime Gateway、session、frontend/backend contract 等架构 specs。

## 7. 推荐集成顺序与回滚点

1. **固定 PR 基线**：待方案批准和任务激活后，从精确 `efdfa5dc` 建新 integration branch；先验证 PR
   自身门禁。不要在当前 73-commit 分支上原地 merge。
2. **固定领域词汇和 seam**：先明确 Runtime Operation/Interaction 与 application Operation/Workspace
   Interaction 的不同事实源，定义 Tool Broker → Operation executor adapter；形成独立提交/回滚点。
3. **移植 application Operation 核心**：重放 `b9067fe5` 起的 Operation core、OperationScript、authority
   与 host 主题；不携带 Session-bound gateway。先让 runtime-gateway crate 独立通过测试。
4. **移植 exact providers**：适配 MCP、Extension、Setup、Interaction provider；MCP 使用 PR RuntimeThread
   binding/BusinessFrameSurfaceQuery，Extension 保持 Component/Operation 最终合同。
5. **移植 Interaction 与 WorkspaceModule**：加入 Interaction domain/repository/API，新 WorkspaceModule
   provider 直接产出 exact Operation/Interaction；保持旧 Canvas 与旧 WorkspaceModule runtime 文件删除。
   同时将 migrations 放在 PR 0065 后，编号为 0066/0067。
6. **适配 AgentFrame、AgentRun 与 Channel**：将 WorkspaceModule refs、capability/attachment 投影接入 PR
   Business Surface；Channel V2 delivery 改走 runtime mailbox/facade。此阶段重做冲突最集中的 API/AppState
   composition，不复活 RuntimeSession。
7. **Contracts 与 frontend**：合并 Rust contract registry后统一生成 TS/schemas；以 PR AgentRun feed/page 为
   基线接入 Interaction/Extension UI，处理唯一重叠 page test。
8. **Specs 与清理**：重写冲突 specs，使两个 Operation/Interaction 边界可执行；合并 journal；结构化搜索
   并删除残留 Session-bound/Canvas action 入口。
9. **全量门禁**：通过后按上述主题提交。每一步保持可独立回滚，禁止一个巨型 conflict-resolution commit。

对于当前分支提交，建议把 commit 当作行为证据而非全部原样 cherry-pick：Channel 主题从
`dd059247` 起，Operation/OperationScript 从 `b9067fe5` 起，MCP/Extension 分别参考 `9cd7d3e1`、
`412234cb`，Interaction/Workspace 参考 `776349dd`、`4cba7e2e`、`b25db387`，最终旧 gateway 删除参考
`1710e360`。凡触及 PR 已删除的文件都应在新 API 上重做。

## 8. 验证门禁

建议按阶段执行，最后再跑全量：

```powershell
cargo metadata --no-deps
pnpm migration:guard
cargo run --bin agentdash-server -- migrate

pnpm contracts:generate
pnpm contracts:check

cargo test -p agentdash-agent-runtime -p agentdash-agent-runtime-host
cargo test -p agentdash-application-agentrun -p agentdash-application-runtime-gateway
cargo test -p agentdash-workspace-module -p agentdash-infrastructure -p agentdash-api

pnpm frontend:check
pnpm frontend:test

cargo check --workspace --all-targets
cargo test --workspace
pnpm check:quick
```

Migration 至少验证空库完整执行，并验证一份代表性预研数据能够完成 0061–0067；重点检查 0065 删除
RuntimeSession 表后，0066/0067 不再引用旧字段。行为验证需覆盖：

- AgentRun launch/follow-up/interrupt/interaction response 的 RuntimeThread binding 与幂等 operation receipt；
- Tool Broker direct callback/MCP facade 的同状态机、approval、retry 与 terminal evidence；
- MCP/Extension/Interaction exact Operation 的 principal、placement、re-admission 与 cancellation；
- AgentFrame → Business Surface → WorkspaceModule/Interaction 的投影；
- Channel ingress/service admission → runtime mailbox → AgentRun event/feed；
- generated TS/schema 与前端 runtime feed/Interaction UI 的同源性。

## 9. 复现命令

```powershell
gh pr view 93 --json number,title,state,isDraft,headRefName,headRefOid,baseRefName,baseRefOid,mergeable,mergeStateStatus,url,commits,files
git fetch origin refs/pull/93/head:refs/remotes/origin/pr-93
git merge-base 7070f6b0 efdfa5dc
git rev-list --left-right --count 7070f6b0...efdfa5dc
git log --left-right --oneline 7070f6b0...efdfa5dc
git diff --name-status -M 957fa9d6 7070f6b0
git diff --name-status -M 957fa9d6 efdfa5dc
git diff --numstat 957fa9d6 7070f6b0
git diff --numstat 957fa9d6 efdfa5dc
git merge-tree --write-tree --name-only --messages --merge-base 957fa9d6 7070f6b0 efdfa5dc
```
