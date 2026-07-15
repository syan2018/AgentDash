# 工作项与并行推进方案

本任务保持一个主任务，不创建child task。四个工作项按可交付能力划分，而不是按crate机械拆分；每项都必须带着main-reference行为、production composition与数据库不变量完成。

## WI-1 Runtime / Native 单一事件链与命令终结

目标：恢复从Runtime command到Native Agent Core再到journal的main等价消息和工具presentation，并让driver acceptance与business terminal成为两条清晰生命周期。

主要搬运与重构：

- 从`D:/Projects/AgentDash-main-reference/crates/agentdash-executor/src/connectors/pi_agent`搬运并适配session item identity、stream role filtering、tool result readable-ref identity、typed tool presentation容错和连续tool loop语义；保留新Runtime contract坐标，不搬回旧connector orchestration。
- 在`agentdash-agent-runtime-contract`与`agentdash-integration-api`补齐effective presentation route、runtime/presentation/source坐标和operation correlation。
- 在`agentdash-integration-native-agent`恢复User/ToolResult过滤、Assistant/Reasoning/Tool/ContextFrame等全事件映射、shared identity、缺省参数容错与exactly-one producer。
- 在`agentdash-infrastructure::agent_runtime_composition`与Native driver把receipt登记前移到Core接受点；接受后的mapper/tool/sink失败写terminal并ack delivery，不重投整个prompt。
- 修复OperationTerminal、TurnTerminal、cancel/abort/follow-up状态机，保证已完成旧operation不会被后续错误改写为Lost。

主要代码所有权：

- `crates/agentdash-agent-runtime-contract/src/{command,driver,event,ids}.rs`
- `crates/agentdash-integration-api/src/agent_runtime.rs`
- `crates/agentdash-integration-native-agent/src/{driver,mapping,presentation,tool}.rs`
- `crates/agentdash-agent-runtime/src/{tool_broker,runtime}.rs`
- `crates/agentdash-infrastructure/src/agent_runtime_composition.rs`

关闭条件：User/ToolResult不生成AgentMessage；每个工具只有一套stable presentation lifecycle；tool轮后继续final assistant；accepted operation恰好一个terminal；post-acceptance错误不增加prompt副作用次数。

## WI-2 Platform Tool逐调用上下文与AgentFrame owner surface

目标：将“可调用工具定义”与“本次调用的业务上下文”分开，恢复六类工具、权限/VFS和不可变provenance的真实接线。

主要搬运与重构：

- 参考main每轮`ExecutionContext`组装语义，将当前provision阶段冻结`DynAgentTool`改为`PlatformToolRegistration + owner executor`；ToolBroker执行时根据request坐标解析typed `PlatformToolExecutionContext`。
- 从canonical binding/thread/turn/frame恢复Hook Runtime、Task scope、workspace module visibility、VFS/mount、permission grant、credential和pending action；移除`surface-bootstrap-*`进入executable handle的路径。
- 对VFS、Workflow、Collaboration/Companion、Task、Wait、Workspace Module六类provider逐一改为消费同一typed context，并各自运行至少一条真实工具。
- 恢复strict surface closure：capability/VFS/HookPlan缺失时typed provision failure；恢复frame-scoped grant/VFS gate，使deny在owner副作用前发生。
- 如现有schema无法表达不可变launch evidence、orchestration/node/attempt或binding generation，直接设计migration并更新恢复查询。

主要代码所有权：

- `crates/agentdash-application-agentrun/src/agent_run/{context_sources,frame,runtime_surface_update}.rs`
- `crates/agentdash-api/src/bootstrap/agent_runtime_surface.rs`
- `crates/agentdash-agent-runtime/src/tool_broker.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/{agent_runtime_composition_repository,tool_broker_repository}.rs`
- `crates/agentdash-infrastructure/migrations/`
- 六类production RuntimeToolProvider及其owner executor实现。

关闭条件：`task_read`与`workspace_module_list`使用真实session/frame成功；六类工具无bootstrap identity；grant/VFS deny前置；残缺surface无法binding；restart/rebind后provenance与generation可恢复。

## WI-3 Native / Codex / Remote connector协议桥接

目标：三类connector都只负责协议和坐标转换，共享Platform ToolBroker业务语义，并按binding能力选择唯一presentation producer。

主要搬运与重构：

- Native：直接复用WI-1的main-equivalent vendor stream；ToolBroker执行平台工具并保留internal state，不发布第二张卡。
- Codex：对齐workspace中的Codex `0.144.1`；标准App Server item/tool notification保持标准body和null/optional语义，dynamic tool/MCP/command callback转到公共Broker，AgentDash扩展只存在于owned wrapper。
- Remote：RuntimeWire携带完整canonical/source coordinates、binding generation与tool-set revision；根据真实offer/profile选择VendorStream或ToolBroker fallback。
- effective presentation route从ToolContribution经profile/binding求交进入persisted catalog、DriverToolDefinition、driver mapper和Broker journal；每个binding只有一个producer。
- Native暂未实现的专属能力在profile中保持unsupported/悬空；Codex标准能力直接接通。

主要代码所有权：

- `crates/agentdash-integration-native-agent/`
- `crates/agentdash-integration-codex/`
- `crates/agentdash-integration-remote-runtime/`
- `crates/agentdash-relay/src/runtime_wire.rs`
- `crates/agentdash-api/src/relay/runtime_wire.rs`
- `crates/agentdash-local/src/handlers/runtime_wire.rs`
- workspace Cargo/Codex lock与generated protocol artifacts。

关闭条件：三种connector的真实production binding均证明single producer；Codex标准body与0.144.1 fixture一致；remote replay受generation fence；公共Broker之外不存在connector私有Task/VFS/permission业务规则。

## WI-4 main组合Oracle、真实数据库与Session前端验收

目标：建立能阻止同类回归的纵向门禁，并用它关闭WI-1至WI-3，而不是在实现结束后补几条孤立单测。

主要搬运与重构：

- 从pinned main-reference提取完整连续事件组合fixture：User → Assistant/Reasoning → 多工具start/update/result → 业务错误 → provider继续 → final Assistant，并覆盖ContextFrame、compaction、rewind、error和cancel。
- 新建production composition测试，装配真实AgentFrame、surface source、六类tool provider、Broker、Native/Codex/Remote driver和embedded PostgreSQL；禁止mock source替代目标装配。
- 在数据库断言journal、projection、tool call、operation、turn、binding、outbox/mailbox与terminal effect；用本次失败run的序列形状作为反例fixture。
- 经真实Session API把eventstream交给现有`sessionStreamReducer`与card registry，断言每个逻辑工具一张卡、顺序与main一致、第二轮和final assistant继续出现。
- 更新四份Runtime规范与Backbone协议规范，使single presentation producer、typed invocation context和post-acceptance terminal成为长期合同。

主要代码所有权：

- 各Runtime/connector crate的composition与integration tests
- `crates/agentdash-api/src/routes/lifecycle_agents.rs`
- `packages/app-web/src/features/session/model/sessionStreamReducer.test.ts`
- `packages/app-web/src/pages/sessionParityEvidence.test.tsx`
- `.trellis/spec/backend/agent-runtime-*.md`
- `.trellis/spec/cross-layer/backbone-protocol.md`

关闭条件：protected `BackboneEvent` body/content/order/null语义与pinned main组合oracle一致；真实PostgreSQL无active悬挂和重复dispatch；现有前端不改业务行为并形成正确单卡；差异矩阵全部关闭。

## 并行调度

1. WI-1与WI-2立即并行：二者先共同固定`DriverToolDefinition / ToolCallCoordinates / PlatformToolExecutionContext`接缝，随后分别拥有driver事件链与owner tool surface文件。
2. WI-3的connector审计与fixture可同时启动；依赖公共contract的代码在接缝稳定后接入，避免各connector自行发明上下文或identity。
3. WI-4从第一天并行建立oracle和数据库harness，并按WI-1至WI-3每次能力闭环持续加断言；最终由它统一验收，而不是等待最后才开始。
4. 所有agent共享当前工作区，开始前声明文件所有权；对共享contract文件采用短时串行窗口。Cargo复用单一target，遇锁等待并协调测试批次，不建立额外worktree/target副本。

## 提交计划

1. `fix(agent-runtime): 恢复 Native 单一事件链与命令终结`
2. `fix(agent-tools): 恢复 AgentFrame 工具执行上下文`
3. `fix(connectors): 收束 Native Codex Remote 工具桥接`
4. `test(session): 建立 main 等价组合链路门禁`
5. `docs(runtime): 固化 Session 与工具边界契约`

每次提交只在对应工作项production composition断言通过后形成；不会以mapper局部测试或单句对话作为提交完成条件。
