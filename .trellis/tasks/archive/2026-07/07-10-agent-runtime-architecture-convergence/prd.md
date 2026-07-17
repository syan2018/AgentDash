# Agent Runtime 架构收敛重构

## Goal

从 Agent 核心能力的实际使用路径出发，重新评估并收敛项目中的 Agent Runtime 架构，使 application、executor、业务 Agent 能力、通用 Agent Core 与各协议适配层之间形成职责明确、依赖方向稳定、可以独立演进的模块边界。

本任务不以保留既有模块划分或协议处理方式为目标。项目尚处于预研阶段，可以基于完整证据推翻既有实现并执行大规模重构，最终状态的正确性与整洁性优先于兼容旧结构。

## Background

- 用户观察到上下文压缩、上下文构造等业务能力可能散落在 application、application-agentrun 或其他模块中，导致业务语义与流程编排耦合。
- 用户期望 application 依赖稳定的通用 Agent 抽象，由 executor 层衔接具体执行能力；内部业务 Agent 与外部 Agent 应在 executor 边界上具备一致的能力表达。
- 业务 Agent 能力需要覆盖项目扩展后的 Codex App Server Protocol 所承载的完整会话语义，包括会话、上下文构造、压缩以及相关生命周期能力。
- Agent 服务需要直接成为可插拔的 Integration 类型，而不是继续由 executor/application 硬编码具体 connector；目标模型必须允许后续接入企业内部自研 Agent 服务。
- `AgentConnector` / Runtime driver 的支持范围需要形成清晰、可查询、可验证的层级或 capability profile，使产品层能准确区分单轮执行、有状态会话、交互控制和平台托管上下文等保证。
- 平台 Hook 需要在目标架构中区分业务 policy authority、runtime orchestration、Tool Broker interception、Agent inner-loop callback、vendor-native materialization 与 observed reaction，不能继续由一个 HookRuntime/boolean 模糊承载。
- Codex 等 Agent 可以通过 native lifecycle hooks接入平台策略；callback + steer只能作为部分触发点的弱化交付，必须在capability中显式区分。
- ACP 首期不作为 Agent Runtime Driver 或 L2 adapter实施；只保留其作为外部消费者接收部分会话状态的可选read-side projection词汇评估。
- 当前协议分层与实际处理归属也在本次评估范围内，现有协议边界不视为既定约束。
- 上述现状判断需要通过代码、测试、规范、历史任务和调用链调查逐项验证。

## Confirmed Facts

- 当前 `AgentConnector` 只在输出 `BackboneEnvelope` 上统一内部 Pi Agent、外部 Codex 与 relay executor；三者的命令集合、thread/turn 生命周期、上下文所有权、恢复和 compaction 语义并不统一。
- `ConnectorCapabilities` 无法表达 `thread/resume/read/fork/compact`、`turn/interrupt`、approval/user-input 交互、context ownership 与 snapshot fidelity，application 因而通过 special service/port 和 connector 类型知识补齐能力。
- `application-runtime-session` 同时拥有 launch/restore、runtime registry、event persistence、context projection、compaction checkpoint、hook/runtime delegate、tool assembly 与 live fanout；`application-agentrun` 又通过 concrete service 或 pass-through bridge 反向调用这些能力。
- compaction 的触发策略、replacement baseline、transcript restore 和成功状态机属于 managed Agent conversation/runtime 语义；infrastructure 只应实现原子存储、CAS、数据库约束与 migration。
- 当前 native compaction 会先替换 core 内存 context，再通过 executor mapper 发送 projection commit 事件；turn processor 会忽略该事件的持久化错误，因此 checkpoint 失败后 turn 仍可能完成，live context 与重启恢复可能分叉。
- 普通 runtime event append 与 active projection head 推进不是同一事务；head 采用 read-modify-unconditional-upsert，存在 append 成功但 head 未推进以及并发 head 回退风险。
- `application-agentrun` 与 `application-runtime-session` 各自定义了 launch/restore classification，且对 compact-only cold restore 的处理已经不同；外部 connector 的 `SystemContext` restore 路径没有完整模型上下文数据流。
- `agentdash-agent` 仍包含 AgentDash Lifecycle 摘要 prompt/回看索引和 runtime compaction policy interface；`agentdash-agent-types` 直接依赖 Codex wire protocol，现有 Agent Core 并非干净 core。
- Codex protocol Rust types 固定在 `rust-v0.140.0`，adapter 启动的 npm app-server 固定为 `0.124.0`；adapter 还会把结构化 input/context 拍平成文本，并未接入完整 thread/turn/interaction 能力。
- Cloud relay 与 local runtime 各运行一套 session runtime，并把 typed Backbone envelope 转为 JSON Value 后再反序列化；relay 不是 canonical Agent Runtime protocol 的透明 transport adapter。
- 项目已有启动期 Host Integration registry，`AgentDashIntegration` 也暴露 `agent_connectors()`；但 Pi/Relay/Codex 仍分别在 API/local composition root 硬编码构建，first-party connector integration 只是非功能占位，因此 Agent 服务 Integration 化尚未闭环。
- 当前 Integration API 直接返回巨大的 `AgentConnector`，contract crate 也反向依赖 `agentdash-spi`/domain；注册阶段只有 executor ID 冲突检测，没有 definition/instance、配置 schema、credential refs、protocol/capability revision、driver factory、session binding 或 conformance guarantee。
- 当前 Hook pipeline 已有“loop外解析、loop边界执行”、AgentFrame control target、Rhai sandbox、HookTrace disposition等正确业务知识；但 `HookRuntimeAccess` 同时混合snapshot、policy、trace、pending action、turn notice、token stats、compaction fuse与capability cache，大量运行态仍为进程内事实。
- 当前 Hook error会在Core delegate、Hub route等路径上偶然表现为fail-closed或fail-open，多个SPI方法又提供默认成功/no-op；required security/completion hook缺少统一failure policy与durable effect语义。

## Confirmed Product Decisions

- Agent服务以可插拔Integration contribution注册、配置和实例化；Native Pi、Codex App Server与未来企业内部自研Agent通过同一Driver扩展点进入Runtime Router。Relay作为独立placement transport contribution接入，不成为Agent service。
- 沿用项目 canonical taxonomy：Integration 是受信、编译期、宿主级扩展，不引入 dylib/WASM 动态原生代码加载；企业仓只追加 Integration crates/企业 binary，不复制宿主装配。运行期可管理的是 Agent service instance、endpoint、配置与 credential refs。
- 无需重编译宿主的远端企业 Agent 通过 AgentDash-owned wire Runtime Protocol 接入，由受信的通用 remote-runtime Integration 承载；不把远端服务误建模为宿主内动态插件。
- 不再以硬编码 connector 枚举、字符串 connector id 或 Composite connector 的能力并集作为 executor 发现和路由机制。
- 目标架构必须明确 Integration contribution/definition、Agent service instance、配置 schema、凭据引用、driver factory、runtime descriptor 与 thread binding 的所有权和边界；Integration 本身保持受信编译期装配，不另造动态 installation 生命周期。
- Runtime 支持范围必须对 application/UI 可见，并由 conformance tests 验证；“接口上存在同名方法”不等于具备同等级的语义保证。
- Level 不只作为抽象能力标签，还应尽量对应可落地的协议能力形状：L4重点复用`references/codex`完整App Server Protocol词汇；L2保留为AgentDash-owned conversation reference class，不绑定某个外部协议。
- 常规多轮AgentRun选择满足durable conversation continuation guarantee的runtime；L1仍可用于一次性workflow/activity，但不是多轮continuation的等价实现。
- 外部 Agent 的工具与项目 feature 集成必须如实分级：区分 driver 原生支持、平台能够保持语义地适配、以及无法注入/无法保证的能力；不能因平台拥有 Tool/VFS/MCP/Capability Pack 就默认外部 Agent 可消费这些能力。
- 本次重构优先级是清理module ownership、依赖方向、状态事实源和adapter seam，而不是提前冻结一套面向完全不受控第三方生态的永久协议边界。
- 企业Agent Core、driver与宿主仍在团队可协调修改范围内，因此Codex/Level/Profile主要作为当前能力词汇、adapter边界和conformance测试基线；允许后续协同演进，不建设不必要的兼容层或认证平台。
- Hook业务authority保留在Business Agent Surface/Managed Runtime；Agent-specific inner hook由Integration Driver Adapter接入，并通过逐trigger `HookProfile`协商。
- ACP首期不进入Driver Host、RuntimeBinding、Relay wire或L1-L4；未来只有真实外部viewer需求出现时，才评估授权脱敏后的BestEffort read-side projection。
- 重构交付在单个父任务下按`workstreams/`目录管理，不创建独立Trellis子任务，避免顶层任务与归档碎片化。

## Requirements

- 完整盘点 Agent Core 的直接与间接消费者、入口、调用链、状态所有权和跨层数据转换。
- 盘点 application、application-agentrun、executor、内部业务 Agent、外部 Agent、Agent Core 及协议适配层的当前职责与依赖方向。
- 追踪会话创建与恢复、消息与事件流、上下文构造、上下文压缩、工具调用、取消与错误、持久化及运行时状态投影等关键链路。
- 评估每一项 Agent 能力的领域归属，区分应用用例编排、业务 Agent 语义、执行器适配、协议传输和纯 Agent Core 能力。
- 建立内部业务 Agent 与外部 Agent 在 executor 边界上的统一能力模型，同时允许项目扩展后的 Codex App Server Protocol 表达完整能力。
- 将 Agent 服务纳入可插拔 Integration 系统，定义 Integration contribution/definition、Agent service instance、配置与凭据、driver factory/activation、protocol negotiation、runtime descriptor、thread-to-driver binding 和隔离边界。
- 设计 `AgentConnector` / Runtime driver 的支持层级；评估严格递进 level 与正交 capability profile/guarantee 的关系，并确保低层级实现不会被误当成完整 managed Agent runtime。
- 完整盘点 `references/codex` App Server Protocol 的 canonical 词汇、方法、notification、server request、生命周期、配置/runtime surface、thread/turn/item、approval/user-input、tools、skills/apps/MCP、compaction、review与错误模型，并建立其与目标 L4 Runtime Contract 的逐项映射。
- 调研ACP作为外部session presentation projection的适用性，明确可投影的message/thought/tool/plan数据与无法表达的turn terminal、operation、interaction、context和durable cursor；首期不实现ACP Driver或projection。
- 建立外部 Agent feature/tool integration 矩阵，至少覆盖：平台托管tools、driver原生tools、MCP、VFS/workspace、structured/multimodal input、system/developer/additional context、Capability Pack/Skill、permission/approval、steer/interrupt、context read/restore/compaction、hooks/mailbox/AgentFrame surface。
- 建立Hook分层模型，定义HookDefinition、HookRequirement、HookPlanSnapshot、HookProfile、BoundHookPlan、HookRun、execution site、failure policy与durable effect语义。
- Agent Hook能力必须逐触发点声明actions、semantic strength、scope、configuration boundary与acknowledgment；不采用`supports_hooks=true`。
- Codex Adapter优先通过隔离的native hook bridge/materialization连接平台Hook Engine；不把业务Rhai规则散落生成到用户项目脚本，也不默认修改项目`.codex/hooks.json`。
- callback/steer只可声明`BoundaryAdapted`或`ObservedOnly`；需要同步block/rewrite/approval、pre-provider变换、pre-compaction cancel或same-loop stop gate时必须有真实pre-action decision channel。
- 对每项外部能力标明 `Native / HostAdapted-Exact / HostAdapted-Boundary / Observed / PromptOnly / Unsupported(current)` 语义、保证强度、所需协议操作、是否可进入 common profile，以及无法满足时对 AgentRun availability/UI 的影响；`Evolvable` 只说明可协同升级路线，不驱动运行时可用性。
- 协议与 Integration 设计遵循“结构边界清晰、语义合同可演进”：第一阶段用 typed seam、descriptor 与仓库内共享行为测试固化会影响正确性的核心不变量；长期生态治理留到出现真实跨团队版本治理需求时再设计。
- 让 Integration 的能力声明、实际 runtime 行为和统一 conformance suite 保持一致；不支持能力必须在产生副作用前返回 typed unsupported，而不是 fallback、静默忽略或退化为普通 prompt。
- 重新设计必要的模块边界、公共契约、依赖方向和运行时数据流，不受现有模块结构限制。
- 明确哪些既有抽象应保留、下沉、上移、拆分、合并或删除，并为结论提供代码证据。
- 形成可分阶段执行、每阶段可独立验证的重构计划；在父任务内以带显式依赖的工作包目录管理，避免不可验证的一次性迁移和顶层任务碎片。
- 不引入兼容层、双轨实现或为旧 API/数据库字段保留的回退路径；如最终设计涉及数据库结构，必须包含正确的 migration 方案。
- 在设计阶段识别并讨论仍需用户决策的产品语义、风险边界和取舍，不用仓库可回答的问题打断用户。

## Proposed Direction

以下方向由三套独立 Runtime seam 设计比较，并结合 Codex、Hook、ACP projection与外部 Agent feature专项调研收敛；正式方案见`design.md`：

- Application/AgentRun 使用具名 `AgentRunRuntime` facade；facade 本身不保存 runtime 状态，只把产品命令映射到通用 Runtime。
- 通用 Business/Managed Agent Runtime 对 application 暴露 `execute / snapshot / events` 三入口，并独占 canonical Thread/Turn/Item、operation journal、availability/admission、context 构造、tool surface、compaction、Interaction 和 terminal 状态机。
- 平台能力拼装由Managed Runtime内部Business Agent Surface完成：Application source adapters提供product facts，编译为immutable `AgentSurfaceSnapshot`；Driver Host把Agent service实际保证归一为`RuntimeOffer`；Runtime admission求交生成并持久化`BoundAgentSurface`/`RuntimeBinding`，Adapter只materialize并回报`AppliedAgentSurface`。
- Integration/Executor Host 位于 Managed Runtime 下方，负责受信 Integration definition/factory、Agent service instance/offer、driver descriptor、placement、durable binding 和 native ID mapping；不拥有 AgentRun 产品语义或 compaction policy。
- 所有 first-party/enterprise Agent service 使用同一个 Integration driver contribution；Relay 只是 remote placement transport，最终 binding capability 为 service guarantee、transport guarantee 与 host policy 的交集。
- `AgentConnector` 业务抽象退役；connector 一词只保留给 L0 transport adapter。L1 Turn、L2 Conversation、L3 Interactive、L4 Managed Thread 仅作为可读的参考类别，不建成永久 trait 继承或单一 admission 门槛；正交 capability profile/guarantee 才是 command availability 的事实源。
- Compaction 完全归属 Managed Agent Runtime，使用 candidate durable commit、driver 幂等 activation、active head CAS 和 crash recovery 的 saga；Agent Core 不持有 AgentDash compaction policy或持久会话事实。
- AgentDash 拥有 canonical Runtime Contract/Wire；Codex、Relay 和企业协议都是 adapter。Backbone 收敛为 typed Runtime Event presentation，AgentRun product events 保持独立事实源。
- L4 Contract 优先以 Codex App Server Protocol 的完整词汇和操作形状作为参照，而不是重新发明另一套相似命名；AgentDash-owned types 仍保持vendor隔离，并为durable context projection/activation补充明确扩展。
- L2 ConversationRuntime由AgentDash-owned guarantees定义，不绑定ACP；具体企业协议按durable continuation/read fidelity行为测试声明。
- Runtime level与项目feature exposure解耦：一个L2/L3 runtime可以拥有稳定conversation/interaction，却仍不支持平台工具注入、Capability Pack或精确AgentFrame context；具体availability由正交profile/guarantee决定。
- Codex≈L4是执行adapter设计参照，不是永久继承关系；owned Runtime Contract应允许在不破坏module ownership的前提下调整level组成、扩展协议操作或协同修改企业Agent Core。
- Hook policy sources由Business Agent Surface编译为immutable HookPlanSnapshot；Managed Runtime、Tool Broker与Driver Adapter按BoundHookPlan分站执行，HookRun/effect回到同一runtime journal/outbox。
- ACP若未来实现，只能位于canonical Runtime Snapshot/Event Stream之后的授权、脱敏、可重建read-side projection Integration；不参与Driver Host、RuntimeBinding、L1-L4或Relay wire。
- 第一阶段conformance只作为共享行为测试与descriptor真实性校验；不要求建立证书发行、复杂evidence持久化或独立生态治理系统。

## Acceptance Criteria

- [x] 现状报告覆盖 Agent Core 到 application、executor、内部/外部 Agent 与协议层的实际依赖图和关键运行时序列，并包含可核验的文件与符号证据。
- [x] compaction、上下文构造、会话状态及协议处理的当前所有权和重复/泄漏点均被识别，结论区分事实、推断与待决策项。
- [x] 目标设计定义各层职责、允许的依赖方向、公开契约、状态事实源、事务与失败边界以及跨层数据转换位置。
- [x] 内部业务 Agent 与外部 Agent 的统一能力模型能够表达扩展 Codex App Server Protocol 的完整会话与压缩语义，并说明能力发现及不支持能力的处理方式。
- [x] 目标设计把 Agent 服务定义为可插拔 Integration contribution，覆盖企业自研 Agent 的注册、service instance 配置、凭据、driver activation、能力协商、运行时路由、thread binding 与隔离语义，不依赖硬编码 connector 分支。
- [x] 目标设计明确区分平台期望的`AgentSurfaceSnapshot`、service实际能力`RuntimeOffer`、admission结果`BoundAgentSurface`与adapter应用回执`AppliedAgentSurface`，required contribution不能静默降级。
- [x] Runtime reference class与capability profile有明确guarantee、availability映射和conformance方式；Native、Codex、企业Agent service以及Relay transport分别套入正确模型。
- [x] 目标设计从第一原则论证模块存在的必要性；不能由明确职责或技术不变量支持的既有抽象被删除或合并。
- [x] 重构范围形成父任务内有序工作包，每个工作包都有显式依赖、迁移目标、验证方式、风险点和完成条件。
- [x] Hook设计明确Business Agent Surface、Managed Runtime、Tool Broker、Driver Host、Agent Core/Adapter与Infrastructure各自ownership，并能阻止weak callback/steer被误报为exact hook。
- [x] Codex native hook bridge覆盖可映射触发点、trust/materialization、configuration boundary、decision translation与HookRun correlation；unsupported actions按typed profile关闭。
- [x] ACP不进入首期Runtime Driver/Relay/Binding实现；可选projection明确为授权脱敏后的BestEffort read model，并说明首期跳过原因。
- [x] 涉及持久化模型变化时，设计包含数据库 migration、数据一致性和验证方案，但不保留旧字段兼容路径。
- [x] `prd.md`、`design.md`、`implement.md` 通过用户评审后才进入实现阶段。

## Out of Scope

- 本轮规划获批之前不修改生产代码，不启动大规模实现。
- 不以维持现有模块名称、公共接口或数据库字段兼容性为目标。
- 首期不实现ACP Agent Driver、ACP L2 adapter或ACP session projection endpoint。
