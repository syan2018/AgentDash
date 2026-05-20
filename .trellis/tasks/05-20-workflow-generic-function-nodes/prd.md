# 工作流通用功能节点扩展

## Goal

扩展现有 Workflow / Lifecycle 系统，让 Lifecycle DAG 除了 `agent_node` 与 `phase_node` 之外，可以承载由平台直接执行的通用功能节点。首批节点聚焦：

- API 请求节点：在工作流中发起 HTTP 请求，并把响应映射成后继节点可消费的 artifact。
- Bash 执行节点：在绑定 workspace 的本机运行环境中执行命令，并把 stdout / stderr / exit code 映射成 artifact 与执行日志。

目标是让工作流可以表达“Agent 思考 / 人工判断 / 平台动作”混合编排，而不是把所有确定性操作都包进 Agent session 中。

## Confirmed Facts

- 现有系统采用两层模型：`WorkflowDefinition` 定义单 session 行为契约，`LifecycleDefinition` 编排多个 step 组成 DAG。
- `LifecycleStepDefinition.node_type` 当前只有 `agent_node` 与 `phase_node`。`agent_node` 会创建独立 child session，`phase_node` 会在已有 session 内切换 workflow contract。
- Lifecycle Edge 已区分 `flow` 与 `artifact`：`flow` 表达控制依赖，`artifact` 表达 port 级数据依赖，并隐含 node 级依赖。
- Step 已有 `input_ports` / `output_ports`，并且后端校验 artifact edge 引用的 port 必须存在于 step 级 ports。
- Port output 已有落点：`lifecycle://artifacts/{port_key}`，底层通过 `InlineFileOwnerKind::LifecycleRun` 的 inline file 持久化。
- 前端编辑器已经有 DAG 画布、Step Inspector、node type 选择、ports 编辑和 workflow contract 编辑。
- 后端 orchestrator 当前只处理两类 Ready 节点：为 `AgentNode` 创建 session；为 `PhaseNode` 激活并刷新运行时能力状态。
- 云端 / 本机双后端架构要求：云端代码不直接访问本地文件系统；本机代码不直接读写业务数据库。

## Requirements

### Functional Requirements

- 支持新增一类平台执行节点，推荐命名为 `function_node`，在 `LifecycleStepDefinition` 中通过 typed spec 区分具体功能类型。
- 首批 `function_node` 支持 `api_request` 与 `bash_exec` 两种 function kind。
- Function node 与现有 Agent / Phase 节点共享 DAG 语义：
  - 入边依赖满足后进入 Ready。
  - 执行成功后写入 output port artifacts 并推进后继节点。
  - 执行失败后节点进入 Failed，Lifecycle 状态可见为 failed / blocked。
- API 请求节点至少支持配置 method、url、headers、query、body、timeout、response artifact 映射。
- Bash 执行节点至少支持配置 command、cwd 选择、env、timeout、stdout/stderr/exit_code artifact 映射。
- Function node 必须记录结构化执行日志，包含开始、完成、失败、耗时、摘要和脱敏后的请求/命令信息。
- Function node 的输出必须进入现有 Lifecycle artifact 通道，使后继 artifact edge 可以复用现有 input port 注入机制。
- 前端编辑器需要在 Step Inspector 中提供对应配置面板，不再把 function node 伪装成需要 workflow contract 的 agent step。
- 生命周期运行视图需要能区分 Agent / Phase / Function 节点，并展示 function node 的执行结果摘要。
- Bash 执行必须走本机运行环境，基于 workspace binding / backend routing 解析执行目标。
- API 请求的执行位置需要在设计中明确，首版推荐由云端执行普通 HTTP 请求；当请求依赖 workspace、本机网络或本机 secret 时再路由到本机。

### Non-Functional Requirements

- 使用强类型 schema 表达 function node 配置，避免继续扩大 `serde_json::Value` 的无约束使用。
- 新增能力不考虑历史兼容和回退；项目处于预研期，直接演进到正确模型。
- 执行过程要可观测：运行日志、错误摘要、输出 artifact、节点状态必须能从现有 UI / API 查到。
- Bash 执行必须有 timeout、工作目录约束和输出截断策略，避免卡住 orchestrator 或无限写入日志。
- API 请求必须有 timeout、响应大小限制和 header 脱敏策略。
- 新增数据库迁移仅在需要新增持久化字段时加入；如果 function spec 落在已有 JSONB `steps` 内，则以 schema / serde 演进为主。

## Acceptance Criteria

- [ ] `LifecycleNodeType` 支持表示平台执行节点，且 Rust / TypeScript 类型保持一致。
- [ ] `LifecycleStepDefinition` 能保存并校验 function node 的 typed 配置。
- [ ] 后端 workflow catalog 校验 function node 的必填配置、port 映射和不适用字段。
- [ ] Orchestrator 能在 Ready function node 出现时自动执行节点，写入 output artifacts，并调用现有 lifecycle 推进逻辑。
- [ ] API 请求节点能执行一个 JSON HTTP 请求，并把 status、headers、body 或选定 JSON path 写入 output ports。
- [ ] Bash 执行节点能在绑定 workspace 的本机后端执行命令，并把 stdout、stderr、exit_code 写入 output ports。
- [ ] Function node 失败时节点状态为 Failed，execution_log 中包含失败原因，后继节点不会被激活。
- [ ] 前端 DAG 节点和 Step Inspector 能创建、编辑、保存 `api_request` / `bash_exec` 节点。
- [ ] 生命周期运行视图展示 function node 状态和执行摘要。
- [ ] 覆盖后端 domain / application 测试、API DTO 序列化测试、前端类型/编辑器测试。
- [ ] 通过项目约定的质量检查：Rust 相关测试、前端 `pnpm` 类型检查 / 测试，以及必要的迁移验证。

## Out of Scope

- 条件分支、循环、补偿事务、显式 join policy，本任务只复用当前 DAG all-complete join 语义。
- 通用低代码表达式引擎。
- Secret 管理完整产品化；本任务只定义 secret 引用位与脱敏要求。
- 长时间运行的后台任务、人工审批节点、cron / trigger 节点。
- 将 function node 暴露为 Agent tool 的产品策略；本任务先解决 Lifecycle 自身的确定性执行节点。

## Open Questions

- 首版 API 请求节点是否只允许云端执行，还是同时支持选择本机后端执行？

## Notes

- 推荐先按 `function_node + FunctionNodeSpec` 的统一抽象规划，而不是直接把 `api_request_node` / `bash_node` 做成两个并列 node type。这样后续新增 JSON transform、变量赋值、文件操作等确定性节点时，可以复用执行器注册、配置面板和日志模型。
