# 通用工作流脚手架近期落地策略

## 文档目标

这份文档用于把 `03-19-symphony-case-workflow-scaffold-closure` 从“高质量讨论 task”推进到“可拆实施 task 的收口前状态”。

它不重复 `prd.md` 里的问题定义，而是回答三个更实际的问题：

1. 当前项目已经具备哪些可复用底座
2. 接下来最值得先落哪条主航道
3. 这个 task 还需要补哪些产出，才能放心归档

## 当前项目基线

结合当前代码与近期任务，AgentDash 已经有以下可复用基础：

- `Project / Story / Task / Session` 的主体模型已经稳定存在
- `Project / Story` 级 `context_containers`、`mount_policy`、`session_composition` 已有正式前后端承接
- `address_space` 已能派生运行期 mounts，并注入到 session runtime
- `Project Session / Story Session / Task Session` 已形成三层正式会话入口
- `session_plan` 已能输出 persona、workflow、runtime policy、tool visibility 等结构化上下文
- Trellis 已经在仓库内真实承担“任务上下文选择 -> 实施 -> 检查 -> 记录 -> 归档”的研发流程职责

这意味着当前真正缺的已经不是“再多一点上下文能力”，而是把这些能力收束成一个真正的 `Workflow` 产品对象与 `Run` 运行对象。

## 当前最关键的判断

### 1. 不要继续把主线放在外围连接器

`external_service`、更多 resolver、更多 provider 都有价值，但它们不是当前最能验证 AgentDash 产品方向的主线。

如果没有一条真实 workflow 主航道，这些能力只会继续堆在执行底座层，无法证明平台层是否成立。

### 2. 第一条主航道应是 `Trellis Dev Workflow`

当前最值得先跑通的真实 workflow，不是 Symphony issue loop，也不是某个外部系统驱动的自动化流程，而是：

- `Trellis Dev Workflow`

原因：

- 它已经是团队真实使用的工作方式
- 它天然覆盖上下文准备、阶段推进、检查、记录、归档
- 它最容易检验 `Workflow / Run / Role / Context / Record` 是否能闭环
- 它可以在不引入外部系统耦合的前提下验证平台产品价值

### 3. 本 task 的价值是“拆任务”，不是继续扩讨论

这个 task 的剩余目标不应是继续补更多抽象名词，而应是：

- 明确第一批实施 task
- 明确这些 task 的依赖顺序
- 明确当前 task 的归档条件

## 推荐落地顺序

### 阶段 A：先把 Workflow 变成平台对象

优先落地：

- `WorkflowDefinition`
- `WorkflowAssignment`
- `WorkflowPhase`
- `WorkflowTarget`

这一阶段先解决“Workflow 在平台里如何存在、如何被 Project 收纳、如何按角色分发”。

暂时不要一上来做完整自动调度 loop。

### 阶段 B：跑通 `Trellis Dev Workflow` 黄金路径

在已有对象之上，先支持一条可人工触发、可阶段推进、可记录产物的 workflow run。

这条路径最小必须覆盖：

1. `Start`
   - 识别目标对象
   - 收集 required reading
   - 生成 phase-specific context
2. `Implement`
   - 挂接 implement context
   - 进入执行会话
3. `Check`
   - 挂接 check context
   - 触发 finish-work / review 类动作
4. `Record`
   - 生成 session summary
   - 产出 journal / archive suggestion

### 阶段 C：再补 `Run / Attempt / Owner Session`

当黄金路径跑通后，再引入真正长期自动化所需的运行态对象：

- `WorkflowRun`
- `WorkflowAttempt`
- `StoryOwnerSession`

这一阶段才开始把“companion session”推进到“owner runtime”。

### 阶段 D：最后补 Control Plane / Workspace Lifecycle / Observability

只有前面三步跑通后，才值得引入：

- `AutomationControlPlane`
- `ManagedWorkspaceLifecycle`
- `WorkflowObservabilitySnapshot`

否则会在没有真实 workflow 主航道时，过早建设复杂运行时。

## 建议立即拆出的实施任务

### 1. `trellis-workflow-platformization`

目标：

- 把 Trellis 当前散落在文档、脚本、jsonl 上的工作流元素提炼成平台对象映射

核心产出：

- Workflow 定义字段
- Workflow phase 模型
- Context binding 规则
- Record / archive policy

### 2. `workflow-definition-and-assignment-model`

目标：

- 明确 Workflow 如何作为全局共享资产被保存、版本化、分配给 Project / Agent Role

核心产出：

- WorkflowDefinition schema
- WorkflowAssignment schema
- Project 与 Workflow 的关系边界

### 3. `trellis-dev-workflow-golden-path`

目标：

- 在产品和运行时里真正跑通第一条 workflow 主航道

核心产出：

- 可显式启动的 workflow run
- 阶段推进
- 阶段级上下文注入
- 记录与归档建议输出

### 4. `story-owner-runtime-closure`

目标：

- 明确 Story owner session 与 companion session 的分层，并补齐 owner runtime 职责

### 5. `automation-run-and-attempt-state-model`

目标：

- 给长期自动化流程建立 run / attempt / retry / due / claim 语义

## 当前不建议优先推进的方向

- 直接追 Symphony 外部系统兼容
- 直接实现自动轮询和长期后台 loop
- 先做复杂 observability 大盘
- 在没有 Workflow/Run 模型前继续扩 session surface
- 把更多 provider / resolver 当成当前主线

## 本 task 的归档条件

满足以下条件后，本 task 可以归档：

- 已至少创建前 3 个实施 task
- `Trellis Dev Workflow` 被明确为第一条黄金路径，而不是继续停留在口头共识
- `Workflow / Run / Role / Context / Record` 的最小边界已有独立文档承接
- 后续实施责任已经转移到子任务，不再需要本 task 继续承载主讨论

在此之前，这个 task 仍应保留为“框架收口 task”，但不应继续无限扩写。
