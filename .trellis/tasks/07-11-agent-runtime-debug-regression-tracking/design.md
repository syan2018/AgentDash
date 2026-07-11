# Agent Runtime 调试回归修复设计

## 1. Execution Profile 控制面

用户选择的是产品级 execution profile，而不是已经实例化的 Runtime offer。控制面从 Integration definition 与平台产品契约投影稳定 profile identity、显示名称、能力边界、availability 和 unavailable reason；首次 AgentRun provision 再根据 profile、Provider、模型、credential scope 与 placement 创建 instance/offer/binding。

API 恢复前端当前需要的 discovery 与 discovered-options 能力，但实现的数据源必须是新的 Runtime definition registry、LLM Provider catalog 与业务 execution profile projection，不恢复 Connector discovery。前端展示不可用项并说明原因，不通过过滤制造空列表。

## 2. 全局 Provider 与 OAuth 授权

全局 Provider credential 属于平台配置，不属于发起 OAuth 的用户。短时 OAuth flow 仍绑定发起 identity 以防止 flow 劫持，但 identity 由 API 的 AuthProvider 解析：Personal 模式允许无 Bearer header 并得到 local identity；Enterprise 模式必须由真实 access token 和 admin 权限通过服务端裁决。

Desktop bridge 的 token 因此是可选传输字段。Web 与 Tauri 不提前判断登录状态；存在 token 时附加 Bearer，不存在时不发送 Authorization。API 保留 `CurrentUser`、`require_system_access` 与 flow owner 校验，最终根据 target 将凭据保存到 global provider 或 user BYOK 的正确存储位置。

## 3. 验证边界

- API/前端测试覆盖 execution profile 可见性、不可用原因与 Provider/model options。
- Desktop OAuth 测试覆盖无 token 不附 Authorization、有 token 正确附加 Bearer。
- API auth 测试覆盖 Personal 无 token、Enterprise 401、Enterprise non-admin 403 与 admin 成功。
- `pnpm dev` 验证 ProjectAgent 配置与首次 AgentRun，以及 openai_codex 全局 Provider 的桌面 OAuth 启动链。

## 4. RunLaunchProfile

ProjectAgent 决定稳定的 executor/Integration identity，并提供默认运行参数。Create AgentRun 不接收 executor override，而以 `model_selection`、`runtime_options` 和 `backend_selection` 分别表达本次运行的模型选择、executor 内部参数与 placement intent；thinking level 属于模型推理选择，与 Provider/model/agent variant 一同进入 `model_selection`。admission 将这些意图与 defaults 编译成 effective execution profile，在 Runtime provision 前写入 AgentFrame revision。Native profile 用 provider/model/identity 构造 service instance；Codex profile只匹配 Codex definition 的 activated offer。Backend selection随 mailbox command进入 provision request并过滤 Host offers；最终 binding记录真实 offer、generation与source thread。

## 5. Pre-provision AgentFrame Business Surface

Lifecycle dispatch 创建的 launch-anchor AgentFrame 是首次 Runtime provision 的业务输入，因此必须在进入 `AgentRunRuntimeProvisioner` 前完成 ProjectAgent owner surface materialization。该 revision 从 canonical Project/workspace resolution、Project VFS mounts、ProjectAgent knowledge/capability directives、MCP 与其他业务 source 生成 execution profile、VFS、capability 和 context surface；Runtime surface compiler只读取并校验该 immutable revision。

surface materialization 属于 Application/AgentFrame construction 边界，不属于 Driver Host，也不由 Native/Codex adapter临时补齐。default mount 的 root/provider/backend/capability必须来自 VFS service 的正常 Project resolution。真实 Project 没有可用 workspace/mount时，construction/admission 返回精确错误；不得把进程 cwd、任意 backend 或空 mount作为替代。

## 6. Tool Capability Provenance

平台内嵌 tool 的 capability ownership 来自 `agentdash-spi::platform::tool_capability` 的 canonical descriptor registry；`CapabilityState.tool_policy` 只表达当前 capability 内的 include/exclude policy。Runtime surface compiler使用 descriptor取得 tool name、capability key与cluster，再用 current AgentFrame capability state校验该路径是否可见。这样 schema/catalog provenance 与执行 admission共享同一 identity，同时保留未知工具和未授权路径的 fail-closed 行为。

## 7. HookPlan 与 offer admission 收口

当前产品路径仍跨过了已有 `AgentSurfaceCompiler` 的 HookPlan compilation：API compiler从 live Hook snapshot取 instruction，却另行硬编码 Driver bindings，导致 Business requirement、execution site 与 Integration profile 三套事实漂移。终态应由 Application 在 AgentFrame construction 时把 Hook policy sources 编译为 immutable HookPlan revision/ref/digest/requirements；Runtime materializer只把其中选定为 Driver/AgentCoreCallback 的 route投影到 `DriverHookSurface`，Managed Runtime/ToolBroker route不进入 Driver action requirement。

过渡修复必须保持相同方向：Driver binding只声明 callback真正承担的同步 block/rewrite/approval/result语义；effect持久化归 Managed Runtime。Native adapter按 bound failure policy执行 fail-closed或fail-open，并让 profile由同一 capability函数生成。Runtime provision对 inventory offer与新 activation offer统一运行 surface admission，避免不兼容事实延迟到 Host bind才暴露。

## 8. Cutover Route Ledger

架构切换不能以替换 router 文件作为完成标志。AgentRun 产品面需要一张可执行 ledger，逐项记录前端 service方法、HTTP route、application query/command owner、generated contract与验收测试。Runtime command/event/context由Managed Runtime facade提供；Lifecycle/AgentFrame identity、subject、workspace surface与模型配置仍由产品 projection组合，但不得再把已退役RuntimeSession当执行状态权威。

事件读取必须区分durable replay与transient live delivery。durable event使用单调cursor支持重连；transient event只有拥有稳定identity/sequence并由live subscription广播时才能跨批次重连消费。有限batch API不得把无cursor transient history在每次轮询中重复返回给同一projection。
