# Trellis Dev Workflow 黄金路径

## 文档目标

这份文档聚焦一个非常具体的问题：

> 如果 AgentDash 只能先落地一条真实 workflow，`Trellis Dev Workflow` 应该如何被平台化，并分几步跑通？

这里讨论的是“第一条真实 workflow 的最小闭环”，不是长期自动化全量设计。

## 为什么是 Trellis

当前选择 Trellis 作为第一条黄金路径，不是为了兼容 `.trellis/` 目录结构本身，而是因为它已经天然包含：

- 启动会话
- 识别当前 task
- 收集 required reading
- 区分 implement / check / debug context
- 记录 session
- 归档已完成 task

也就是说，它已经是一条真实存在的团队工作流，只是目前主要以文档、脚本和约定形式存在。

## 黄金路径要验证什么

`Trellis Dev Workflow` 至少要证明以下几点：

1. Workflow 可以是平台一等公民，而不只是 prompt 模板
2. Context 可以按 phase 稳定注入，而不是靠 agent 临场猜测
3. Run 可以围绕目标对象推进多个阶段，而不只是一次 session
4. 记录与归档可以成为 workflow 输出，而不是额外手工动作

## 最小产品对象

第一版不需要把所有长期自动化对象一次做完，但至少应具备这些最小对象：

### 1. `WorkflowDefinition`

负责描述：

- workflow 名称
- 适用目标类型
- phase 列表
- 每个 phase 的上下文规则
- completion / record / archive policy

### 2. `WorkflowAssignment`

负责描述：

- 某个 Project 可使用哪些 workflow
- 某类 agent role 默认绑定哪个 workflow

### 3. `WorkflowRun`

负责描述：

- 某个 workflow 针对某个目标对象的一次运行实例
- 当前 phase
- 当前状态
- 最近一次 session / action / record 输出

### 4. `WorkflowPhaseState`

负责描述：

- phase 是否已进入
- phase 是否完成
- phase 输出了哪些 artifact / decision

### 5. `WorkflowRecordArtifact`

负责描述：

- summary
- journal 更新建议
- archive 建议
- phase completion note

## 第一版目标对象建议

建议第一版先支持以下目标对象：

- `Story`
- `Task`

其中：

- `Story` 更适合承接 owner / companion 级流程
- `Task` 更适合承接 implement / check 切片

第一版可以不急着把 `Project` 作为完整 workflow target，只需保留设计余地。

## Phase 映射建议

### Phase 1: Start

平台动作：

- 识别当前目标对象与当前 task
- 加载 workflow definition
- 收集 required reading
- 生成初始 context set

对应当前 Trellis 经验：

- `/trellis:start`
- `get_context.py`
- 读取 spec / workflow / task PRD

### Phase 2: Implement

平台动作：

- 绑定 implement context
- 打开或复用执行 session
- 记录当前执行目标与约束

对应当前 Trellis 经验：

- `implement.jsonl`
- task start / coding / verify

### Phase 3: Check

平台动作：

- 切换到 check context
- 执行 review / checklist /质量确认
- 生成问题或通过结论

对应当前 Trellis 经验：

- `check.jsonl`
- `finish-work`
- `check-backend` / `check-frontend`

### Phase 4: Record

平台动作：

- 汇总本次 run 的关键产出
- 生成 journal 记录
- 给出 archive suggestion

对应当前 Trellis 经验：

- `add_session.py`
- `record-session`
- `task.py archive`

## 建议的交付切片

### Slice A：先做“定义与映射”

目标：

- 明确 Trellis workflow 的平台对象映射

必须产出：

- WorkflowDefinition 草案
- phase 与 context 的映射表
- record / archive policy 草案

这一阶段可以先不改运行时代码。

### Slice B：再做“可启动的 WorkflowRun”

目标：

- 让某个目标对象可以显式启动 `Trellis Dev Workflow`

必须产出：

- `WorkflowRun` 持久化对象
- 当前 phase 状态
- phase 切换入口

这一阶段先允许人工推进 phase，不要求自动化 loop。

### Slice C：接入 Session 与 Context 注入

目标：

- 让不同 phase 能绑定不同 session context

必须产出：

- Start phase required reading 注入
- Implement / Check phase 的 context switch
- Session UI 中的 phase 可见性

### Slice D：接入 Record 与 Archive

目标：

- 让 workflow 的结束产物能真正反哺平台

必须产出：

- session summary artifact
- journal update suggestion
- archive suggestion

第一版可以先生成建议，不强制自动执行。

## 与现有能力的衔接方式

当前项目里已经能直接复用的部分：

- `Project / Story / Task / Session` 模型
- `session_plan` 与结构化上下文摘要
- `context_containers / mount_policy / session_composition`
- `Project Session / Story Session / Task Session`
- Trellis 的任务目录、PRD、jsonl context、journal 记录约定

因此第一版应尽量做“组合与提炼”，而不是重写一套平行体系。

## 第一版明确不做什么

- 不做长期后台自动调度 loop
- 不做外部 tracker 驱动
- 不做复杂 retry / claim / reconciliation
- 不做完整 managed workspace lifecycle
- 不做全量 observability 控制台

这些属于后续长期能力，不应阻塞第一条黄金路径。

## 完成信号

当以下结果出现时，可以认为 `Trellis Dev Workflow` 黄金路径成立：

- 平台里存在正式 `WorkflowDefinition`
- 某个目标对象可以显式启动 `Trellis Dev Workflow`
- run 能在 `Start / Implement / Check / Record` 之间推进
- 不同 phase 能稳定拿到对应上下文
- run 结束后能产出记录与归档建议

一旦这条路径成立，后续再引入 `owner session`、`run / attempt`、`control plane` 就会有真实锚点，而不是抽象空转。
