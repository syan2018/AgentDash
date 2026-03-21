# 长期实现 Symphony 类流程目标跟踪

## Goal

单独跟踪 AgentDash 后续如何逐步具备承载 Symphony 类长期自动化流程的能力，把它作为一条长期目标线持续推进，但不把它与“通用工作流脚手架 / 产品模型讨论”混在同一个 task 里。

这个 task 的目标不是直接实现 Symphony，也不是逐条对齐其 SPEC，而是持续回答：

- 为了承载一类长期自动化流程，AgentDash 还需要补齐哪些框架能力？
- 哪些能力应先通过移植 `Trellis workflow` 来落地验证？
- 哪些能力适合先停留在抽象设计层，哪些需要尽快形成真实闭环？

## Scope

本 task 只负责跟踪“长期实现 Symphony 类流程”这条目标线，重点覆盖：

- workflow contract
- workflow 作为全局共享资产的产品模型
- owner / worker session 运行模型
- automation control plane
- managed workspace lifecycle
- workflow run / attempt / retry / reconcile 语义
- runtime snapshot / observability
- Trellis workflow 的平台化迁移路线

## Non-Goals

- 不负责承接所有关于 AgentDash 工作流产品定义的讨论
- 不负责 Linear / GitHub / Jira 等外部系统适配细节
- 不要求短期内形成一条可商用的 Symphony 兼容实现
- 不把 Symphony 当作 UI / 产品形态模板
- 不把某个脚本目录结构或命令行表面形式直接当成平台核心模型

## Relationship With Other Tasks

### 与 `03-19-symphony-case-workflow-scaffold-closure` 的关系

两者关系如下：

- `03-19-symphony-case-workflow-scaffold-closure`
  - 偏“产品与框架抽象讨论”
  - 关注 AgentDash 作为通用 workflow scaffold 到底缺什么
  - 关注 Workflow / Story / Run / Task / Context 这些产品对象

- `03-20-symphony-flow-long-term-tracking`
  - 偏“长期目标跟踪与落地路线”
  - 关注如何逐步把这些抽象变成真实能力
  - 关注 Trellis 迁移、owner runtime、control plane、run state 等实现路线

两者可以互相引用，但不要混成一个任务。

## Strategic Positioning

当前对 Symphony 类流程的正确态度应是：

- 把它当成长期能力目标
- 把它当成框架完备性的验收样例
- 不把它当成当前产品路线的唯一主角

换句话说，这个 task 是“长期目标看板”，不是“当前迭代主线”的唯一来源。

## Product Repositioning

### AgentDash 不应只是 Agent 执行器集合

经过前面的讨论，当前更合理的产品理解是：

> AgentDash 不是“支持很多 agent 的技术框架”，而应该是“让团队把工作流交给 agent 持续运行、观察、干预和复用的操作台”。

这意味着产品重心不应该停留在：

- 能启动多少种 agent
- 能挂多少个 tool
- 能读多少个 mount

而应该转向：

- 工作流如何定义
- 工作流如何在项目里被分配和复用
- 工作流运行中如何看见、介入、恢复
- 团队上下文如何成为平台资产并稳定共享给 agent

### AgentDash 的更清晰定位

当前更值得追求的定位是：

- 一个统一的 Agent 执行与编排脚手架
- 一个可容纳不同 workflow contract 的 runtime
- 一个可共享团队上下文、项目上下文、进度上下文的平台
- 一个能把 Story / Task / Session / Workspace / Context / Validation 组合成长期工作流的系统

## Symphony 在这里的角色

Symphony 只承担两个作用：

- 暴露“长期自动化 workflow”需要哪些底层能力
- 帮我们发现 AgentDash 的框架缺口

它不承担以下作用：

- 不定义 AgentDash 的最终 workflow DSL
- 不定义 AgentDash 的 issue model
- 不定义 AgentDash 的外部系统边界
- 不定义 AgentDash 的 UI 或产品表面形态

## Current Working Judgment

### 1. 近期最值得先跑通的真实 workflow

不是直接做 Symphony 兼容，而是：

- 先把 `Trellis workflow` 作为第一条真实 workflow 移植进 AgentDash

原因：

- Trellis 已经是一条真实在用的研发流程
- 它天然包含上下文、阶段、检查、记录与归档
- 它比单纯的 Task 执行流程更能验证 AgentDash 的平台价值
- 它比直接接某个外部 tracker 更接近我们当下真正掌控的工作方法

### 2. Symphony 类流程的长期落地顺序

建议按以下顺序推进：

1. 先验证 `Workflow` 能不能作为平台一等公民存在
2. 再验证 `Trellis workflow` 能不能跑成一条真实主航道
3. 再补 `Story owner runtime`
4. 再补 `Automation Control Plane`
5. 最后才考虑接入某类外部事件 / issue-driven 自动调度

### 3. 当前最容易误入的方向

当前最需要避免的是：

- 一上来追求对齐 Symphony 的外部系统集成
- 在没有 `Workflow / Run / Owner Session` 模型前就做自动化 loop
- 继续只围着底层能力打转，而没有明确第一条工作流主航道

## Workflow As A Product Asset

### 讨论结论

Workflow 不应该只是某次 prompt 的配置，而应该是平台中的全局资产：

- 全局维护
- 支持团队共享
- 支持版本化
- Project 下可收纳多个 workflow
- 可按 Project 下的 agent 行为 / 角色进行分发

### Workflow 的责任

一个 Workflow 至少应表达：

- 目标对象类型
  - Project / Story / Task / Session companion / 其他
- agent persona / role
- 必须上下文
- 运行阶段
- 阶段性动作
- 验证与交接规则
- 记录 / 归档策略
- 可选的 timeout / retry / hooks / confirmation policy

### 不应把 Workflow 降格成什么

不应把 Workflow 简化成：

- 一段 prompt 模板
- 一组脚本命令
- 一组外部 tracker 集成规则

这些都只是 Workflow 的部分载体，不是其产品定义。

## Project / Workflow / Role / Run / Story / Task 心智模型

结合前面的讨论，更合理的产品对象关系如下。

### Project

长期上下文容器，负责：

- 收纳多个 workflow
- 管理项目上下文与进度上下文
- 给不同角色的 agent 分发不同 workflow

### Workflow

可复用的方法模板，定义：

- 如何理解目标
- 如何使用上下文
- 如何推进阶段
- 如何产出记录与沉淀

### Agent Role

Project 下承担某类职责的 agent 行为，不等于具体模型，也不等于某个单独 session。

典型角色包括：

- Project context maintainer
- Story lifecycle companion
- Task execution worker
- Review / check agent
- Record / archive agent

### Run

某个 Workflow 作用于某个目标对象的一次运行实例，是长期自动化流程最关键的运行态对象。

### Story

目标单元，围绕某个明确目标持续推进。

### Task

执行切片，是 Run 在推进过程中拆出来的工作单元，而不应成为整个产品的唯一主角。

### Session

Run 的运行媒介。不同 Session 可扮演不同角色，例如：

- owner session
- worker session
- companion session
- review session

## Context As A First-Class Product Value

### 讨论结论

AgentDash 不只是管理任务，也应该管理团队上下文，用于记录、留档，并作为平台把上下文稳定共享给 Agent。

因此，上下文在产品中不应只是技术附件，而应是正式价值点。

### 应被管理的上下文类型

- 项目上下文
- 进度上下文
- Story 上下文
- Task 上下文
- 会话记录
- 审查与归档记录

### 这对长期自动化流程的意义

Symphony 类流程的真正难点，不是“轮询工单”，而是：

- 如何在长期运行中不丢失上下文
- 如何把上下文沉淀回平台
- 如何让后续 agent 继续消费这些上下文

而这恰好是 AgentDash 应该比 Symphony 更有优势的地方。

## Why Trellis First

### 不是为了兼容 Trellis 的脚本形式

移植 Trellis workflow 的目的，不是把 `.trellis/scripts/*.py` 和 slash commands 原样搬进平台。

真正要迁移的是 Trellis 背后的工作方法：

- Session Start
- Task Context Selection
- Required Reading
- Implement
- Check / Review
- Record / Archive

### Trellis 之所以适合作为第一条真实 workflow

因为它天然覆盖了：

- 阶段化 workflow
- 上下文准备
- 当前任务切换
- 规范注入
- 检查与收尾
- journal / 记录 / 留档

它比“单纯跑一个 Task”更接近 AgentDash 想成为的平台，也比“直接做 Symphony 兼容”更贴近当前掌控范围。

## Trellis Workflow 的平台化抽象

### 不应直接照搬的部分

- 不把 Python 脚本本身当平台抽象
- 不把 `.trellis/` 路径结构硬编码成平台核心模型
- 不把 slash command 语法当产品主交互
- 不把 jsonl 文件格式本身当平台规范

### 应提炼成平台能力的部分

- `Task Context Source Set`
- `Required Reading Rule`
- `Phase Transition Action`
- `Session Record Policy`
- `Completion Checklist`
- `Workflow Assignment`
- `Role-specific Context Injection`

## First Golden Path

### 讨论后的当前建议

如果当前只能先跑通一条 workflow，不建议直接做“Symphony issue loop”，而建议优先形成以下主航道：

- `Trellis Dev Workflow`

并将其作为：

- 第一条真实 workflow 迁移对象
- 后续 `Story Companion Workflow` 的母版
- 平台验证 Workflow / Run / Context / Record 能力的第一条闭环

### 这条黄金路径最小应包含的阶段

1. `Start Phase`
   - 识别当前任务、当前阶段、当前项目上下文
   - 自动收集必读上下文

2. `Implement Phase`
   - 绑定 implement context
   - 允许推进开发

3. `Check Phase`
   - 绑定 check context
   - 提供 finish-work / check-backend / check-frontend 等检查动作

4. `Record Phase`
   - 生成 session 摘要
   - 更新 journal / 记录 / 归档建议

### 为什么这条黄金路径重要

因为一旦它跑通，就能同时证明：

- Workflow 是平台一等公民
- Context 是 Workflow 可控输入
- Record 是 Workflow 可控输出
- Project 可以把 Workflow 分发给不同 Agent 行为

## Long-Term Capability Targets

这条目标线最终希望 AgentDash 具备以下能力。

### 1. Workflow 作为全局共享资产

- 全局维护
- 支持团队共享
- 支持版本化
- Project 下可收纳多个 workflow
- 可按 Project 下的 agent 行为 / 角色进行分发

### 2. Workflow Assignment / Agent Role

一个 Project 下应能明确：

- 哪些 workflow 可用
- 哪个 workflow 绑定给哪类 agent 行为

典型角色包括：

- Project context maintainer
- Story lifecycle companion
- Task execution worker
- Review / check agent
- Record / archive agent

### 3. Owner Session / Worker Session 分层

为了承载 Symphony 类长期流程，必须补齐：

- owner session：持续推进目标
- worker session：执行具体切片

当前 `Story session` 更像 companion session，还不是完整 owner runtime。

### 4. Run / Attempt / Retry 模型

长期自动化流程必须拥有独立于 `Task.session_id` 的运行态模型，至少表达：

- 当前 run 是什么
- 当前 attempt 是第几次
- 为什么进入 retry
- 下一次 due 时间
- 当前 claim 是否仍持有
- 是 waiting / running / paused / released / failed / completed 中哪一类

### 5. Managed Workspace Lifecycle

长期自动化流程最终需要：

- deterministic workspace 选择或派生
- prepare / reuse / cleanup 机制
- workflow 阶段级 hooks
- owner / worker 与 workspace 的明确绑定关系

### 6. Automation Observability

operator 应该能看到：

- 哪些 workflow run 在运行
- 哪些在 retry queue
- 谁是 owner session
- 当前 phase / action
- 最近失败与 reconcile 结果
- token / runtime / resource 概览

### 7. Workflow-Controlled Context Circulation

长期自动化流程不仅要消费上下文，还要能反哺上下文：

- 从 Project context 读取
- 在 Story / Task 执行中更新
- 在结束后沉淀回平台
- 让后续 workflow 或 agent 继续消费

## Current Gaps We Intend To Track

基于当前代码与前面讨论，当前最值得持续跟踪的缺口包括：

- `Workflow` 仍未成为平台正式对象
- `Run` 仍未成为平台正式对象
- `session_composition` 仍不足以表达完整 workflow contract
- `Story session` 仍偏 companion，而不是 owner runtime
- `Task.session_id` 不能替代长期 automation run / attempt 模型
- workspace 仍偏 CRUD 元数据，而不是 managed lifecycle
- observability 仍偏 session stream，而不是 workflow run snapshot

## Near-Term Deliverables

作为长期目标跟踪 task，近期更现实的产出应是：

1. 梳理 Trellis workflow 的平台化迁移清单
2. 梳理 Workflow / Run / Control Plane 的最小模型草案
3. 明确 owner session 与 companion session 的差异
4. 明确哪些能力已经具备，哪些需要拆独立实施任务
5. 输出一版 `Trellis Dev Workflow` 的平台对象映射

## Suggested Follow-Up Task Directions

后续大概率可以从这个长期 task 再拆出以下子任务：

1. `trellis-workflow-platformization`
2. `workflow-definition-and-assignment-model`
3. `story-owner-runtime-closure`
4. `automation-run-and-attempt-state-model`
5. `automation-control-plane-loop`
6. `managed-workspace-lifecycle`
7. `automation-observability-snapshot`
8. `workflow-context-circulation-model`
9. `trellis-dev-workflow-golden-path`

## Acceptance Criteria

- [ ] 该 task 明确与通用 workflow 讨论 task 分离
- [ ] 明确 Symphony 类流程只是长期能力目标，而非当前产品模板
- [ ] 明确 Workflow 应作为全局共享资产存在
- [ ] 明确 Trellis workflow 是第一条真实迁移对象
- [ ] 明确当前第一条黄金路径是 `Trellis Dev Workflow`
- [ ] 明确 `Workflow / Run / Role / Story / Task / Session / Context` 的推荐心智边界
- [ ] 能持续作为后续子任务拆分的母任务存在
