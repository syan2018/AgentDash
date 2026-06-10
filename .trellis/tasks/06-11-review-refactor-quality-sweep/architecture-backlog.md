# Architecture Backlog

本文件只记录架构设计问题。实现级表面质量问题进入 `fixes/`。

## 判定口径

只有满足下列条件之一的问题才进入本文件：

- 预计涉及超过 10 个文件的巨大幅度修改。
- 需要改变跨模块事实源、公共 contract、数据库/migration 或前后端共同消费的协议。
- 无法在单一模块处理单元内完成，需要先做独立设计再实施。

未达到上述门槛的问题，即使体现耦合或职责过宽，也先作为模块级 refactor 候选进入 `review-index.md` 或对应 `reviews/` 文件。

## 条目格式

```text
## ARCH-000: 标题

- 优先级：
- 状态：
- 证据：
- 影响面：
- 建议方向：
- 不在当前模块修复中处理的原因：
```

## 当前条目

## ARCH-001: inline mutation 存在 API 与 Agent runtime 两套语义

- 优先级：P1
- 状态：待设计
- 证据：`crates/agentdash-application/src/vfs/mutation_dispatcher.rs:97` inline 写入直接走 repo/storage key；`crates/agentdash-application/src/vfs/service.rs:310` agent tool overlay 写入走 `InlineContentOverlay`。
- 影响面：VFS API mutation、Agent runtime overlay、inline_fs 持久化、冲突处理。
- 建议方向：收敛为一个 inline mutation port/use case，overlay 只表达 session 暂存层，持久化写入统一经过 dispatcher 或更底层 inline storage writer。
- 保留为架构项的原因：涉及 API mutation、Agent runtime overlay、inline_fs 持久化三套事实语义，预计需要跨 API/application/storage 多模块迁移。

## ARCH-002: workflow ready node 启动链路有两套入口

- 优先级：P1
- 状态：待设计
- 证据：`dispatch_common` 对 graph-backed dispatch 自己创建 session/frame/anchor 并提交 `NodeStarted`；`OrchestrationExecutorLauncher::drain_ready_nodes` 也负责从 ready queue 启动 AgentCall/Function/HumanGate。
- 影响面：workflow dispatch、lifecycle start、orchestration scheduler、NodeStarted runtime event 事实源。
- 建议方向：统一为 `dispatch_common` 只创建/确保 run + orchestration，所有 ready node 启动都交给 launcher；或把 launcher 拆成唯一 scheduler port。
- 保留为架构项的原因：涉及 dispatch service、launcher、runtime reducer、session/frame/anchor 建立链路，预计跨 10+ 文件和关键运行路径。

## ARCH-003: 生命周期状态事实源分散

- 优先级：P1
- 状态：待设计
- 证据：orchestration/run 状态聚合在 reducer 内 `derive_orchestration_status`、`sync_lifecycle_run_status_from_orchestrations`；active run 选择和 projection/view builder 对状态再次解释。
- 影响面：Blocked/Paused/Ready/Running 优先级、scheduler、view、active run selection。
- 建议方向：把状态聚合提升为 domain/application 共享的 lifecycle status projector，所有 view/selection/scheduler 只消费同一 projector。
- 保留为架构项的原因：涉及 domain/application projection、scheduler、view、active run selection 多条路径，必须先定义统一状态优先级。

## ARCH-004: session running/control 状态事实源分散

- 优先级：P1
- 状态：待设计
- 证据：`SessionChatView.tsx:269` 运行态由 `fetchSessionExecutionState` 轮询、raw stream event 扫描、`optimisticRunning` 三者共同推导。
- 影响面：session action availability、runtime control、execution projection、chat UI loading/running 状态。
- 建议方向：收敛到后端 runtime-control / execution projection 作为唯一控制事实源，stream event 只做失效触发，不直接决定 action running。
- 保留为架构项的原因：涉及前后端 runtime-control / execution projection 事实源，不能只在前端局部修补。

## ARCH-005: Session UI 直接消费完整 BackboneEvent

- 优先级：P1
- 状态：待设计
- 证据：`SessionDisplayEntry` 直接携带完整 `BackboneEvent` 穿透到 `SessionEntry.tsx` 和工具卡 registry。
- 影响面：session UI、tool card registry、context frame rendering、generated event contract。
- 建议方向：定义 session feed view model union，例如 `MessageEntry / ToolEntry / SystemEventEntry / ContextFrameEntry`，UI 不直接 switch generated event。
- 保留为架构项的原因：涉及 generated event contract、session feed view model、UI registry 多层边界，预计迁移范围较大。

## ARCH-006: runtime tool composer 完整迁出 VFS

- 优先级：P2
- 状态：待设计
- 证据：`crates/agentdash-application/src/vfs/tools/provider.rs` 中的 `RelayRuntimeToolProvider` 同时装配 VFS、shell、workflow、companion、canvas、workspace module 等工具；runtime provider wiring 参与 API bootstrap 和 session runtime ready gate。
- 影响面：VFS tool factory、session runtime composer、API bootstrap、workflow/canvas/companion/workspace module tool assembly、AppState ready gate。
- 建议方向：先在 VFS 内抽出 VFS tool factory；后续由 session/runtime ownership 下的 `RuntimeToolProviderComposer` 组合各领域 factory，并把 `SessionToolServices` 迁到 session-owned 模块。
- 保留为架构项的原因：完整迁移预计超过 10 个文件，跨 VFS、session launch、API bootstrap、workflow、companion、canvas、workspace module 与 runtime gateway 边界。

## ARCH-007: local-runtime CommandHandler 服务边界过宽

- 优先级：P2
- 状态：待设计
- 证据：`crates/agentdash-local/src/handlers/mod.rs` 的 `CommandHandler` 同时持有 backend/workspace/tool/session/connector/MCP/terminal/materialization/extension artifact 等依赖，`handle()` 仍集中分发 heartbeat、prompt、workspace、tool、MCP、extension、terminal 等 relay command。
- 影响面：local runtime WebSocket command bus、prompt/session forwarder、tool calls、MCP relay、terminal、materialization、extension artifact activation。
- 建议方向：设计真实的 command service 边界，例如 `PromptCommandService`、`ToolCommandService`、`ExtensionCommandService`、`TerminalCommandService`，再由小 router 消费 typed handler services。
- 保留为架构项的原因：有价值的拆分会触及 `handlers/` 大部分文件、`ws_client.rs`、构造/config plumbing 和测试；只包一层 router 无法降低依赖宽度。

## ARCH-008: prompt MCP relay contract 仍是 raw JSON

- 优先级：P2
- 状态：待设计
- 证据：`crates/agentdash-relay/src/protocol/prompt.rs` 与 `crates/agentdash-application-ports/src/backend_transport.rs` 仍以 `Vec<serde_json::Value>` 表达 prompt MCP servers；application producer 与 local parser 需要各自解释 wire shape。
- 影响面：relay protocol、application backend transport port、application relay connector、API workspace resolution、本机 prompt parser，以及所有发送 `command.prompt` 的 backend/local runtime 实现。
- 建议方向：引入明确的 relay prompt MCP DTO，作为 relay/application/local 共同消费的 typed contract，再逐步移除 raw JSON 解释。
- 保留为架构项的原因：这是跨 relay/application/local 的公共 wire contract 变更；当前循环只修复发送形态，不把 raw JSON contract 一次性迁移。

## ARCH-009: Extension Host process/env sandbox contract 未定义

- 优先级：P2
- 状态：待设计
- 证据：`env.get` 有 `env.read[:NAME]` 权限检查，而 process execution 默认继承宿主进程环境；显式 `options.env` 的权限收敛只能覆盖调用方主动传入的 env overlay。
- 影响面：extension manifest permission、process.shell/process.exec 默认环境、Node/PowerShell 命令发现、SDK 文档与样例、插件运行兼容边界。
- 建议方向：如果产品要求强隔离，先定义 process 执行是否 `env_clear()`、注入哪些最小基础环境、以及是否需要新的 env pass/inject 权限族。
- 保留为架构项的原因：完整沙箱会改变公开 extension permission contract 和 runtime 行为，不能混入当前模块级 process executor 收敛。

## ARCH-010: companion platform broker 与权限授权闭环未统一

- 优先级：P1
- 状态：待设计
- 证据：`companion_request(target=platform, payload_type=capability_grant_request)` 当前仍会转成人类 companion request；`PermissionGrantService`、grant lifecycle spec 和 companion capability grant reference 已要求授权事实由 permission grant / capability transition 承载。
- 影响面：companion tool semantics、PermissionGrant 事实源、AgentFrame capability state、runtime tool surface 更新、用户审批 UI/API、CapabilityResolver 可见性。
- 建议方向：定义单一 platform broker use case，负责 grant 创建、policy decision、用户审批 handoff、frame effect application 与 live runtime capability/tool-schema update。
- 保留为架构项的原因：最小模块修复可以先拒绝假授权或接入已有 `PermissionGrantService`，但完整 broker 闭环会跨 companion、permission、session capability service、API route、前端审批视图和 runtime projection。
