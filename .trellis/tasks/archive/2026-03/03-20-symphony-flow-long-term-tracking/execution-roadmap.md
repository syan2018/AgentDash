# Workflow 落地执行文档

## 文档目标

这份文档用于记录当前项目在阅读真实代码后的落地判断，并把第一阶段要做的实现内容收束成明确执行路线。

它回答三个问题：

1. 当前项目已经真实实现到了哪一步
2. 下一阶段最该实现什么
3. 第一批实施 task 之间如何衔接

## 当前项目的真实基线

结合当前代码，项目已经不是“仅有 Task 执行器”的状态，而是具备了明显的 workflow runtime 雏形：

- 已有 `Project / Story / Task / SessionBinding / Workspace` 领域对象
- 已有 `context_containers / mount_policy / session_composition` 的正式领域建模
- 已有 `ExecutionAddressSpace` 与 mount 派生逻辑
- 已有 `Project Session / Story Session / Task Session` 三层正式会话入口
- 已有 `session_plan`，能生成 persona、workflow、runtime policy、tool visibility
- 已有 `PiAgent + RuntimeToolProvider + Address Space tools` 的统一访问底座
- 已有 `Task start / continue / cancel / restart / reconcile` 的执行恢复骨架

这说明项目当前真正缺的，不是更多零散 capability，而是把这些隐式结构收束成显式的：

- `WorkflowDefinition`
- `WorkflowAssignment`
- `WorkflowRun`
- `WorkflowPhaseState`
- `WorkflowRecordArtifact`

## 当前不应继续当主线的方向

以下能力有价值，但当前不应占用主航道：

- 继续扩更多 provider / resolver
- 直接做 Symphony 外部系统兼容
- 提前做完整 automation control plane
- 提前做复杂 observability 大盘
- 在没有 Workflow / Run 模型前继续扩 session surface

这些方向都依赖一个更底层的前提：

> 平台里必须先有正式 workflow 对象，以及至少一条真实 workflow 黄金路径。

## 当前最该实现的内容

当前最值得实现的是：

- 让 `Workflow` 成为平台一等公民
- 跑通第一条真实 workflow：`Trellis Dev Workflow`

原因：

- Trellis 已经是真实使用中的研发流程
- 它天然覆盖 `Start -> Implement -> Check -> Record`
- 它最能验证 `Context / Run / Record / Archive` 是否能闭环
- 它不依赖外部系统，可直接利用当前代码底座完成第一轮平台化

## 第一阶段执行顺序

### 阶段 1：Trellis Workflow 平台化映射

目标：

- 把 `.trellis/workflow.md`、task 目录、jsonl context、journal / archive 这些现实机制，映射成平台对象和 phase 规则

对应 task：

- `03-20-trellis-workflow-platformization`

产出重点：

- Workflow phase 模型
- Context binding 规则
- Record / archive policy
- 现有 Trellis 元素与平台对象的映射表

### 阶段 2：Workflow 定义与分配模型

目标：

- 把 `WorkflowDefinition / WorkflowAssignment` 变成正式领域对象和接口设计

对应 task：

- `03-20-workflow-definition-and-assignment-model`

产出重点：

- WorkflowDefinition schema
- WorkflowAssignment schema
- Project / Workflow / Agent Role 边界
- 版本化、分发、启用规则

### 阶段 3：Trellis Dev Workflow 黄金路径

目标：

- 基于前两步产出，真正跑通第一条 workflow run

对应 task：

- `03-20-trellis-dev-workflow-golden-path`

产出重点：

- `Start / Implement / Check / Record` 四阶段
- phase 与现有 session/context 注入的接线
- record artifact 与 archive suggestion
- 至少一条可人工触发、可阶段推进的 workflow run

## 第一阶段完成后再进入的内容

第一阶段跑通后，再进入第二批能力：

- `story-owner-runtime-closure`
- `automation-run-and-attempt-state-model`
- `automation-control-plane-loop`
- `managed-workspace-lifecycle`
- `automation-observability-snapshot`

这些任务不应该阻塞第一条黄金路径。

## 实施时应复用的既有能力

第一阶段应优先复用，而不是重写：

- `SessionBinding`
- `Project Session / Story Session / Task Session`
- `session_plan`
- `address_space`
- `TaskExecutionGateway`
- `RestartTracker / StateReconciler`
- 前端 Session 上下文解释面

原则是：

- 先做对象收束与 phase 接线
- 不重写 executor / relay / context builder 底座

## 当前执行结论

当前项目最合理的落地策略是：

1. 先把 workflow 从“文档和约定”变成“正式对象”
2. 再把 Trellis 变成第一条平台内 workflow
3. 再补长期自动化运行时

如果跳过前两步直接做长期自动化，项目会继续停留在“能力很多但缺少主航道”的状态。
