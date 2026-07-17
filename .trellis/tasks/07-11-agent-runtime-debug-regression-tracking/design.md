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

## 15. Provider terminal 与 Mailbox/Runtime admission 边界

Responses SSE的逻辑完成由协议terminal event定义，不由HTTP连接生命周期定义。`response.completed`被完整解析后，bridge立刻把已归约的content、tool calls和usage封装为唯一`StreamChunk::Done`并结束producer；transport EOF只用于没有协议终态的异常检测。这样`MessageEnd → AgentEnd → Runtime TurnTerminal`跟随Provider事实，而不是连接池或HTTP/2 body何时关闭。

消息提交分为四个owner。`AgentRunMessageSubmissionService`拥有产品`client_command_id`、canonical request digest、exact response replay和“本次请求只能关联本次mailbox message”的归属；PostgreSQL Submission UoW在同一事务创建Pending product receipt、identity-free mailbox draft和两者关联。Mailbox Queue只拥有delivery/barrier/priority/order、claim lease、status与retention；Delivery Coordinator读取fresh Runtime view、claim并调用统一Runtime ingress；Runtime facade唯一拥有canonical Operation、presentation identity、admission time和accepted duplicate replay。

Mailbox draft只保存UserInputBlock/source/launch语义和隐藏Runtime input，不保存`AgentRunPresentationInput`、admission timestamp或任何turn/item/revision。新Operation被真正接纳时，Runtime facade生成main等价的`t<millis>` launch identity，或绑定当前active presentation turn并生成mailbox steer item；accepted Operation已经存在时直接读取durable command/presentation并严格校验draft、Runtime input、actor和target，不重新生成动态事实。这样排队延迟不会伪造成已启动时间，崩溃恢复也不会因terminal竞争把原Steer误重规划成新的Start。

Promote是Mailbox策略变更，必须经由application owner原子修改policy字段且不隐式Resume。普通stale admission释放claim并保留原draft重新规划；claim lease过期形成typed reconciliation work，优先交Runtime ingress判断稳定Operation是否已经接纳。Repository不通过`delivery_result_unknown`猜测外部副作用。成功接纳只保存Runtime operation ID与真实Started/Steered状态，用户visible payload和隐藏delivery draft按retention policy同一结算清理；execution profile override只作为typed hidden delivery intent存在，不成为Mailbox列；真实turn/presentation identity只从Runtime read model/eventstream读取。

产品幂等identity只由target、input、execution profile/backend选择和delivery intent等稳定业务语义组成，不包含snapshot/stale guard。API完成鉴权与target ownership后先通过Submission owner查询或认领receipt：settled receipt直接exact replay，只有新认领命令才执行mutable guard和delivery转换；无副作用拒绝会条件释放未attach的Pending reservation。因此Runtime状态变化不会让相同HTTP retry在到达receipt前被stale guard误杀，也不会要求客户端用刷新后的guard制造另一个digest。

Execution profile切换不由Mailbox解释。隐藏draft把typed override交给Managed Runtime；stable Operation replay先返回，active turn判定其次，Steered完全忽略override。只有最终选择Started时，`ExecutionProfileCoordinator`读取current AgentFrame并负责Frame revision、service reconfiguration与Runtime realignment。尚未建立binding时，不同profile先形成完整carry-forward的新AgentFrame revision再provision；已有binding且profile相同保持no-op。已有binding的不同profile需要planned service rebind，而现有Provisioner/SurfaceAdopt没有该能力，因此返回typed unsupported，不能把surface-only adoption伪装成service切换。

ProjectAgent start仍需恢复具名application orchestrator，使receipt、Lifecycle graph、initial mailbox、accepted refs和失败cleanup处于同一产品owner。纯input/config/backend/subject转换必须在reservation与launch前完成；launch后的失败不能只靠API catch终态化receipt而遗留orphan graph。

`ProjectAgentRunStartService`只持有产品级initial submission port：ProjectAgent解析、Lifecycle launch与产品projection留在start owner，标准UserInput到Mailbox delivery draft的编译和receipt-message原子attach留在Message Submission owner。attach结果使用`Unattached | Attached | Unknown`显式表达；只有确定未attach且graph没有可见execution event时清理整份run/agent/frame。初始输入与accepted result分别只有一份canonical source，不能同时传递presentation/runtime input或projector/frozen variants两套可分叉数据。

Runtime输入直接承载AgentDash-owned Codex app-server标准`UserInputBlock`，而不是另一套Text/Image/FileReference镜像。Codex adapter原样序列化Text、Image、LocalImage、Skill与Mention，包括nullable detail和text_elements；Structured走typed additional context。Native adapter仍只支持Text/Image，并在side effect前明确拒绝其它标准variant和Structured。API独立验证至少存在一个非空白Text或任意非Text块，但不trim、过滤或重写已接纳的标准块。

API只负责鉴权和DTO映射，不为每个queued请求从`after=None`订阅Runtime历史事件。canonical terminal后的drain由`RuntimeMailboxTerminalConvergence`触发，进程/租约恢复由pending recovery worker负责。Product receipt第一次返回Queued后保持exact Queued replay，即使后台后来消费了该message；调度器推进旧高优先级message时也不能把旧Operation receipt返回给当前提交。

## 16. Runtime Surface exact candidate 与 adopted snapshot

Runtime Surface adoption同时涉及两个不同版本，接口必须显式区分：`expected_active`来自Managed Runtime thread snapshot，`candidate`来自本次AgentFrame surface mutation。candidate Frame ID是compile/adopt operation的必需坐标，不能在compiler内部通过`get_current(agent_id)`或runtime binding重新发现。

`AgentRunRuntimeBinding`只表达Runtime thread、Host binding、driver generation、profile与启动时descriptor等绑定事实。它不是live surface head，原因是普通`SurfaceAdopt`不会创建新的binding lineage，也不会更新该文档。当前adopted descriptor由Managed Runtime `RuntimeThreadState.surface`唯一持有；产品active surface、CAS expected base与recovery均读取这一事实。

目标数据流：

```text
current Runtime snapshot F1
  + exact uncommitted AgentFrame candidate F2
  -> validate candidate closure / compile Business Surface / compile presentation delta
  -> persist immutable F2
  -> RuntimeCommand::SurfaceAdopt(expected=F1 revision+digest, target=F2 descriptor)
  -> commit executable publication
  -> current Runtime snapshot F2
```

Application surface source只加载一次明确Frame并由该Frame派生executor、tools、Hook snapshot与Business facts。旧active surface只作为adoption plan的base输入，不与candidate source facts合并为一个“current”对象。`BusinessFrameSurfaceQuery`继续服务产品/资源查询时，也必须由调用方提供明确adopted descriptor；不得把binding bootstrap descriptor升级为live pointer。

`AgentFrameRepository`保留immutable revision store职责。最高revision只服务revision allocation、历史和诊断；方法命名必须反映latest persisted语义。active读取不进入该repository。由于项目未上线，直接删除错误的`get_current` active语义和冗余兼容分支。

Adopter以stable operation identity协调surface store、tool registry publication与Managed Runtime CAS。所有可确定的candidate closure、tool/hook和presentation compile错误在持久化前暴露；跨系统提交失败时，Managed Runtime snapshot仍是active权威，任何已持久化candidate都只能作为未采用历史，不能被产品查询误认为current。
