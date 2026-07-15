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

## 9. Runtime Turn identity 与 Driver acknowledgement

`TurnStart` command由Managed Runtime创建canonical `RuntimeTurnId`，并通过`DriverCommandEnvelope.runtime_turn_id`把同一identity交给Driver。Driver返回的`TurnStarted`是对既有Turn的acknowledgement；adapter可以同时记录Integration原生source turn，但Tool、Hook与terminal event必须继续引用canonical Runtime turn，二者不能互相代替。

Driver若已经发出`TurnTerminal`，底层Agent任务随后返回同一失败只表示该业务结果已完成投影。dispatch应成功结束并让durable outbox ack该command；只有尚未产生terminal event的派发失败才进入重试/失败处理。该边界保证一次用户Turn只有一份canonical lifecycle，也避免终态command被重复执行。

## 10. Desktop credential claim convergence

Desktop enrollment由native `DesktopRunnerHost`唯一拥有。`POST /api/local-runtime/ensure`必须使用有界connect/request timeout；retryable timeout由native supervisor投影为`waiting_for_api`并按既有retry policy继续，fatal auth/contract错误进入`error`。`claiming`只覆盖实际在途的单次HTTP attempt。

Web bridge只负责在当前用户可用时触发native ensure，不复制native retry state machine。`runtimeStart`返回`running`才可标记auto-connect完成；`claiming`、`waiting_for_api`、`starting`与`retrying`保持未完成，并由后续snapshot/触发继续观察。Backend action仍读取server live registry，只有relay完成registration后才成为online。

验证分为三层：HTTP悬挂的确定性timeout测试、Web中间态不完成测试、真实`pnpm dev:desktop`中ensure→relay registered→目标backend online→Runtime action成功的产品链验证。

本机Runtime拥有独立的持久数据库，因此已发布到任何开发数据库的migration同样不可原地改写。AgentFrame HookPlan的最终列名由0067 rename migration建立；0066恢复为已应用的原始内容，使既有数据库先通过checksum校验再顺序升级。

## 11. Shell terminal typed owner 与 continuation routing

`shell_exec` 的真实进程与 retained output buffer 继续由 local runtime `ShellSessionManager` 持有；API/application 层只保存可寻址的 terminal control registration。VFS runtime tool provider从当前 `PlatformToolExecutionContext` 取得 `run_id`、`agent_id` 和 canonical `runtime_thread_id`，在 start 前注册 terminal_id 对应的 backend、mount、cwd 与 owner scope，control operation再用同一 registration路由到原 mount/backend。

composition root必须把唯一的 `AgentRunTerminalRegistry` 通过 adapter注入VFS provider。adapter写入typed owner，不依赖前端是否已经订阅presentation stream；兼容旧 session反查不作为新路径。start/read/write取得的output snapshot回写同一registry用于terminal projection，但不得替代local runtime retained buffer或成为命令执行事实源。

`VfsRuntimeToolProvider`与`VfsToolFactory`将terminal registry作为构造期必需依赖，使production composition漏线成为编译错误；`ShellExecTool`在start副作用前再次校验registry与typed owner。start结果是完整snapshot，cursor control返回带sequence的增量chunks；adapter按sequence去重追加有界preview，空增量不覆盖既有投影。

验证至少覆盖两层：VFS tool级 start返回running后以terminal_id read到terminal状态与最终输出；production composition级证明VFS provider确实拥有registry且typed owner被注册。失败路径不得返回一个无法续接的running handle。

## 12. Driver event staged reduction 与 revision CAS

`RuntimeRevision`是Thread aggregate的committed版本，不是Driver event pump的临时进度。Driver ingress对一个envelope执行归约时必须同时持有两份语义：从repository加载且与数据库CAS一致的committed base，以及只在内存推进的staged projection。整批fact全部通过transition、terminal projection与write-set校验后，才以committed base revision提交staged projection。

任一fact失败时不得把部分归约后的staged projection交给protocol-violation路径。violation commit从原committed base生成`ProtocolViolation`、active entity lost terminals与quarantine，确保失败批次的前置fact没有journal记录、projection副作用或revision消耗。violation helper显式接收committed base/expected revision，不能从任意传入state推断CAS基线。

critical violation一旦原子terminalize canonical Turn/Operation，ingress admission必须向Driver event pump返回typed terminalized结果，使底层Core立即停止且不再追加另一份`BindingLost`。同一commit必须复用正常Turn terminal的presentation projector与application effect builder，保证canonical、前端eventstream和产品终结副作用观察同一个Lost事实；不能只写internal terminal后向producer报告普通成功。

Native Provider retry已经由Backbone `PlatformEvent::ProviderAttemptStatus`形成完整ephemeral presentation。adapter只发布这一份transient事实；`RuntimeEvent::ProviderStatus`不得作为第二份internal transient summary进入Driver ingress。这样retry/status不推进durable revision，Thread revision只为canonical durable lifecycle、Item、Interaction、Context与presentation journal事实排序。

单Thread revision继续保留，因为canonical lifecycle与durable journal需要一个全序CAS边界；拆分多个revision会把一致性问题转移到跨流合并。稳定性来自纯staged reducer、数据库零部分提交、合法producer的CAS rebase/lease策略与明确的transient边界，而不是删除并发保护。

## 13. Managed Runtime 复杂度审查与收敛形态

相较main-reference由PostgreSQL原子递增`last_event_seq`并直接追加Backbone presentation的薄session链，当前Runtime增加了canonical Thread/Turn/Item/Interaction、Operation/idempotency/outbox、Integration offer/binding/generation、Tool Broker、HookRun和Context checkpoint。Host placement、重绑定与stale generation fencing、跨Integration统一生命周期、重启恢复和durable side effect是这些层级带来的实际收益，不能通过恢复Connector/RuntimeSession第二事实源替代。

过度设计集中在两个边界。第一，presentation-only transient数据同时拥有Backbone事件与AgentDash internal同构镜像，增加producer/admission冲突而不产生恢复收益；前端eventstream继续以main等价presentation内容为唯一展示事实。第二，`RuntimeRevision`同时承担store CAS、aggregate snapshot precondition与近似event cursor的职责，并被surface/context等已有专用revision/digest的内部操作消费，使无关presentation能够制造业务stale。

目标形态保留canonical Runtime与durable Host/Operation能力，但建立唯一per-thread mutation coordinator。command、driver、tool、hook、context和surface提交typed intent；进程内使用keyed per-thread serialization，跨实例以数据库CAS为权威，只有明确的CAS loser执行有界reload/reapply。`EventSequence`只表示journal cursor；whole-thread revision只表示aggregate版本并作为内部并发坐标，surface/context/binding使用各自业务precondition。Driver adapter不复制presentation-only internal事件，前端wrapper可演进但payload行为保持main等价。

`DriverError::Terminalized`在ARD-012中作为event sink到pump的typed flow-control，使现有Native/Codex/Remote contract可以立即停止producer；它不是Driver有权声明的canonical终态。outbox必须回读durable Operation/Thread/binding后才决定ack。ARD-013应把该语义从通用Driver error/wire union收回sink-specific admission/error，缩小外部误用面。

## 14. ARD-012 break-loop 分析

- **B Cross-Layer Contract**：Native mapper产生了Runtime明确禁止的internal transient，而Provider status已有完整ephemeral presentation owner；producer与admission没有成对契约。
- **C Change Propagation Failure**：critical terminal admission没有同步传播到Native、Codex、Remote、durable worker与outbox settlement，局部处理后仍可能继续pump或错误ack。
- **D Test Coverage Gap**：adapter mapper与Runtime transition单测分别通过，但缺少`producer -> admission -> pump -> PostgreSQL UoW`组合回归，未覆盖valid prefix后接invalid suffix。

预防机制已经落到三处：kernel/persistence/adapter code-spec固定committed base与terminalized边界；cross-layer guide要求枚举producer和全部consumer；真实PostgreSQL回归证明末阶段失败全回滚、canonical终态同事务落地和fabricated terminalized不吞active outbox。更广的多producer CAS与revision职责问题由ARD-013继续收束。
