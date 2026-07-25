# Agent 服务 Integration 化现状基线

> 本文补充用户确认的目标：Agent 服务本身必须成为可插拔 Integration，供企业内部自研 Agent 接入；同时需要重新定义 AgentConnector / Runtime driver 的支持层级。本文只记录仓库事实与设计约束，不修改生产代码。

## 1. 结论

项目已经存在一套 Host Integration 启动期装配机制，也已经在 `AgentDashIntegration` 上暴露 `agent_connectors()`，因此目标不是从零发明插件系统。真正的问题是这个扩展点尚未闭环：

- 内置 Pi、Relay、Codex 仍由 API/local composition root 硬编码构建；first-party connector integration 只是明确标注的非功能占位。
- Integration API 直接返回当前巨大且语义不完整的 `AgentConnector`，把 SPI、domain、Codex-shaped payload 和 application launch projection 一并暴露给企业 integration。
- 注册阶段只枚举 `list_executors()` 并检查 executor ID 冲突，没有 manifest、配置 schema、凭据引用、runtime protocol revision、capability guarantee、conformance certification 或 session binding。
- API 宿主支持 `new_with_integrations(...)`，local runtime 却直接构建 Codex + Composite，没有消费同一个 Integration registry；同一个 Agent 服务扩展模型无法跨 cloud/local 组合根成立。
- 当前 Integration 是“编译进企业 binary、启动期注册、更新后重启”的 native integration，不是运行时下载任意动态库或脚本。`.trellis/tasks/06-04-plugin-extension-taxonomy/` 已将该边界确认为 canonical taxonomy，本次设计直接继承，不再把动态原生代码加载列为开放问题。

因此，Agent Runtime 重构应复用 Host Integration 的顶层装配理念，但必须用新的、较窄的 runtime integration contract 替换 `agent_connectors() -> Vec<Arc<dyn AgentConnector>>`，并让所有 first-party Agent 服务也通过该扩展点注册。

## 2. 已有 Host Integration 模型

`.trellis/spec/backend/capability/integration-api.md` 定义：

- 开源仓提供宿主、`agentdash-integration-api` 与 first-party integrations；企业仓只追加企业 integrations 与企业 binary；
- 稳定 contract crate 应保持轻量，不透传 Tokio、Axum、SQLx、Reqwest、RMCP 或具体 executor runtime；
- 装配顺序为“收集 integrations -> registry -> 冲突检测 -> 构建 provider/runtime -> AppState -> 启动”；
- extension ID 冲突必须 fail fast；
- first-party integration 应成为扩展合同的第一个真实消费者，避免企业仓第一个踩中合同缺陷。

这些原则适合直接继承到 Agent Runtime Integration：宿主拥有产品状态、授权、持久化与统一协议；integration 只贡献 descriptor、factory 和 adapter 实现。

当前 `AgentDashIntegration` 位于 `crates/agentdash-integration-api/src/integration.rs:51-169`，可注册 auth、identity、VFS、mount、routine、marketplace、skill、memory 等 providers，也包括：

```rust
fn agent_connectors(&self) -> Vec<Arc<dyn AgentConnector>> {
    vec![]
}
```

`crates/agentdash-api/src/integrations.rs:104-342` 在 API crate 内收集 integration，调用 `on_init()`，汇总 providers，并通过 connector 的 `list_executors()` 检查 executor ID 冲突。`HostIntegrationRegistration` 本身也是 API-private 类型，而不是可被 API/local/其他宿主共同复用的 registry module。

## 3. 当前 Agent 服务仍然硬编码

### 3.1 Cloud/API composition root

`crates/agentdash-api/src/bootstrap/session.rs:288-326` 的实际顺序是：

1. API 自己调用 `build_pi_agent_connector(...)`；
2. API 自己构建位于 application crate 的 `RelayAgentConnector`；
3. 最后追加 `integration_connectors`；
4. 再将所有 connector 包进 `CompositeConnector`。

因此 integration connector 只是硬编码内置 connector 之后的附加项；Agent 服务并未统一由 Integration registry 产生。

### 3.2 Local composition root

`crates/agentdash-local/src/runtime.rs:567-569` 直接构建 `CodexBridgeConnector`，再套一层 `CompositeConnector`。全仓只有 API 的 `new_with_integrations(...)` 消费 `AgentDashIntegration`；local runtime 没有同一注册入口。

这意味着企业自研 Agent 若需要在 local backend 执行，当前不能只实现一次 Integration contract，而要修改另一套宿主 wiring。

### 3.3 First-party integration 仍是占位

`crates/agentdash-first-party-integrations/src/lib.rs:49-70` 的 `ConnectorCatalogIntegration` 明确声明不装配或暴露任何 connector；Pi/Relay/Codex 仍由宿主直接构建。该占位反而证明项目已有“连接器最终应走 Host Integration 装配”的方向，但尚未实现。

## 4. 当前 Integration API 的依赖和 interface 问题

规范称 contract crate 应轻量，但 `crates/agentdash-integration-api/Cargo.toml` 直接依赖 `agentdash-spi` 与 `agentdash-domain`。`agent_connectors()` 进一步将整个 `AgentConnector` 暴露为企业 SPI。

当前 `AgentConnector` 的 surface 同时包含：

- executor discovery；
- live session probe；
- prompt/cancel/steer/approval；
- tool replacement 与 session notification；
- 过宽的 `ExecutionContext`，其中包含 AgentDash domain config、VFS、MCP、hooks、runtime delegates、context frames、restore state 和 application placement facts。

它既不够完整——缺少显式 start/resume/fork/read/compact/interrupt 等 runtime lifecycle；又过度泄漏——企业 Agent adapter 被迫依赖大量内部 DTO。这样的 module 缺少 depth：interface 很大，隐藏的复杂度很少，integration implementation 与宿主 application 的 locality 都很差。

正确 seam 应让企业 integration 只依赖：

- AgentDash-owned Runtime Protocol types；
- Integration/driver descriptor 与 factory context；
- 明确受控的 host services ports，例如 secret resolver、process/transport client factory、observability sink；
- conformance harness。

它不应依赖 application service、repository row、Backbone vendor DTO、Codex type、具体 native Agent Core 或 Composite router。

## 5. Definition、Instance、Driver 与 Binding 必须分离

现有 `AgentDashIntegration::name()` + `agent_connectors()` 无法表达企业 Agent 服务的真实生命周期。目标至少要区分：

| 概念 | 所有者 | 作用 |
| --- | --- | --- |
| Integration Package / Definition | 启动期 registry | 声明 integration key/version、driver kinds、配置 schema、所需 secret slots、protocol revision、静态 capability 上界 |
| Integration Installation / Instance | 平台产品与持久化 | 一份管理员配置的 Agent 服务实例；保存非敏感配置、credential refs、enabled/health 状态和 placement |
| Runtime Driver Factory | Integration adapter | 根据 instance snapshot 与受控 host services 构建/取得 driver；不直接读取平台数据库或环境变量 |
| Runtime Driver | Executor/Runtime seam | 实现统一 session/turn/interaction protocol，并报告经过验证的 descriptor |
| Runtime Binding | Business Agent Runtime / durable store | 将 AgentRun delivery/runtime thread 绑定到 integration instance、driver kind、executor source IDs 与 capability revision |

Native Pi、Codex direct、Relay remote 和企业自研 Agent 都应是这一模型的实例，而不是 router 中的特殊分支。

## 6. 支持“层级”不能只靠单个数字

用户要求清楚划分 AgentConnector 实际支持层级。调查显示严格层级有展示价值，但单个 ordinal 会掩盖正交差异。例如一个 Agent 可能支持 approval/interrupt，却只拥有 opaque context；另一个可以 exact snapshot/restore，却不支持 hot tool update。

建议后续方案比较同时考虑两层表达：

1. **产品可读的 Runtime Class / baseline level**：说明最小闭环，例如 turn-only、stateful conversation、interactive conversation、managed-context runtime；
2. **机器可验证的 capability profile + guarantee**：分别描述 lifecycle、interaction、input/context channels、tool surface、compaction ownership、snapshot fidelity、terminal reliability、transport/reconnect。

Runtime class 只能由一组 guarantee 推导，不能由 integration 自由自报。具体 capability 仍作为路由、availability 与 conformance 的事实源。

一个合理的候选递进基线是：

- `TurnExecutor`：typed turn start，structured terminal，EOF-before-terminal 为 Lost；
- `ConversationExecutor`：在前者上支持稳定 session binding 与 start/resume/read，可选 fork；
- `InteractiveExecutor`：在前者上闭环 interrupt、steer、approval/user-input 与 tool revision；
- `ManagedContextExecutor`：平台可得到 exact materialized snapshot、restore 和 platform-projection compaction，durable checkpoint ack 后才激活 live context。

`NativeOpaqueCompaction`、特定 multimodal channel、fork、dynamic tool update 等仍应是正交 capability，不能为凑等级伪装成完整支持。名称和切分需要等待三套 interface 设计比较后确定。

## 7. 对本次目标架构的直接约束

- 所有 first-party Agent 服务必须和企业 Agent 使用同一个 Integration extension point；不允许 Pi/Codex service 或 Relay transport 在 composition root 留特殊构建分支。
- Integration registry 必须在所有宿主共享；API/local/未来 worker 只提供不同 host service adapters，不重新定义 registration。
- Router 按 `integration_instance_id + driver_id/executor_id` 路由并持久化 session binding；取消、steer、approval 只发给 owner driver，不广播试探。
- Integration descriptor 提供静态上界；driver handshake/instance health 提供运行时实际能力；bound session 固定 capability revision，能力变化必须显式重协商或终止，不可静默漂移。
- Unsupported 必须在产生副作用前返回 typed error；tool update、approval 等不能用默认 `Ok(())` 表达静默忽略。
- Integration package 版本、Runtime Protocol revision、instance config revision、capability revision 与 source session IDs 都应进入 binding/audit。
- first-party integrations 和一个最小示例企业 integration fixture 必须共同运行 Runtime conformance suite，证明企业仓不需要修改宿主 wiring。

### 7.1 Integration、placement 与 transport 是正交轴

进一步检查后需要修正上面“Pi/Codex/Relay 都是 Agent 服务”的简写：

- Native Pi、Codex App Server、企业自研 Agent 是 **Agent Runtime service/driver**，应由 Integration 贡献；
- Relay 是 **placement transport**，不是 Agent 服务，不应继续伪装成 executor/connector；
- LLM provider bridge 是 Native Agent 的下游 provider adapter，也不是独立 Agent Runtime service。

正确解析过程应是：

```text
Agent service instance + runtime variant
  -> backend placement
  -> local driver 或 remote driver proxy
  -> 若为 remote，Relay 透明承载同一 Runtime Protocol
```

Local backend 通过自己的 Integration registry 装配 Codex/企业 driver，再经 Relay handshake 发布 descriptor。Cloud 侧持久化的是同一 service provenance 与远端 placement/binding；不能把它重命名成一个通用 `relay` executor 后丢失原 integration identity。

最终 bound capability profile 应由 service guarantee、transport guarantee 与 host policy 共同裁决并固定 revision。Relay 的存在不能创造 service 本来没有的能力，也不能通过薄 DTO 无声降级 service 已声明的能力。

## 8. 已确认的加载与信任边界

`.trellis/tasks/06-04-plugin-extension-taxonomy/prd.md` 与 `design.md` 已确认：

- `Integration` 是宿主/部署作用域的受信原生扩展，编译期绑定，明确不做动态 dylib/WASM 加载；
- 企业仓通过追加 Integration crates 与企业 binary 跟随 upstream，不维护独立宿主装配；
- `Extension` 与 `Capability Pack` 是数据驱动、可安装内容，不能承载受信 Agent driver 代码。

本次 Agent Runtime 重构必须沿用该 taxonomy。所谓 Agent 服务“可插拔”指：所有 first-party/enterprise driver 使用同一编译期 Integration contract 和注册流程；Integration 贡献的 service definition 及其安装实例、endpoint、非敏感配置和 credential refs 可以在产品运行期管理。

对于不希望重新编译宿主的企业自研 Agent，正确扩展方式是实现 AgentDash-owned wire Runtime Protocol，由一个受信、编译期存在的通用 remote-runtime Integration 将该服务注册为 instance。这样动态变化的是远端 service instance，不是宿主内执行的任意代码，仍保持 Integration 信任边界。
