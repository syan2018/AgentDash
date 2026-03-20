# 梳理 Project 级 Agent 模板与共享上下文迭代

## Goal

围绕 Project 级多 Agent 模板、Project 共享上下文维护、Project 下直接对话入口，以及 Story 默认模板继承关系，梳理一条可持续推进的产品与架构迭代线。

这个任务的目标不是立即落地全部页面和运行时能力，而是先把以下问题收敛清楚：

- Project 应该如何承载多个 Agent 模板及其工作流
- 哪些能力属于“模板/Agent 构建页”的重型配置
- 哪些能力属于 Project 日常使用中的轻量入口
- Project 级共享上下文应如何被维护、复用并传递到 Story / Task
- 哪些 Project 模板会成为 Story 下默认 Agent 模板

## Background

当前项目已经开始显式化：

- `Project / Story` 上下文容器
- `session_composition`
- Session 运行时解释
- Story / Task 维度的 Agent 执行承接

但随着讨论深入，新的业务诉求已经超出了“把后端运行时信息展示出来”这一层。

新的核心约束包括：

1. `Project` 页面原则上需要支持维护多个 Agent 模板及其相关工作流。
2. 模板定义、Agent 构建逻辑、较重的工作流配置，不适合塞进 Project 基础页，后续应有独立的重型页面承接。
3. 在实际使用中，用户会希望直接在 `Project` 下与这些 Agent 对话，用于维护 Project 共享上下文，而不必先进入某个 Story。
4. 部分 `Project` 级模板会成为 `Story` 下 Agent 的默认模板，例如：
   - 协助用户整理需求
   - 收集 Story 关联上下文
   - 帮助把 Project 共享信息映射到具体 Story

这意味着系统已经不只是“Story / Task 执行平台”，还开始需要明确：

- `Project` 级 Agent 资产
- `Project` 级共享上下文
- `Project -> Story` 的默认继承关系
- “配置入口”和“实际使用入口”的分层

## Confirmed Decisions

以下结论已在讨论中确认，后续设计应默认遵循：

### 1. `Project Agent Template` 是独立正式对象

- 不再把它仅视为 `project.config.agent_presets` 的附属字段
- 它应成为挂在 `Project` 下管理的正式产品对象
- 现有 `agent_presets` 可作为过渡兼容实现，但目标模型应是独立对象

### 2. 用户侧默认感知应接近“原生文件系统”

- 实际面向用户时，不应首先暴露 provider、capability、permission 这类底层实现细节
- 用户更自然地把多个 mount 理解为文件系统中的多个文件夹
- 用户应能在对话中方便地引用这些上下文内容，而不是先理解底层挂载机制
- 只有在编排 Agent、排查权限、配置运行时能力边界时，才需要进入更重的实现与权限视角

### 3. 需要 `Project` 级独立会话模型

- `Project` 下直接与 Agent 对话，不应只是 Story / Task session 的变体
- 需要和 Story / Task 平行的 `Project` 级 companion / owner session 概念
- `Project` 级会话的核心职责是维护共享上下文、沉淀项目知识、辅助后续 Story

### 4. Story 创建不强绑定默认模板

- 用户创建 Story 的行为通常是独立的，不应在创建时强绑定 Agent 模板
- 只有当用户在 Story 下主动开启 Session 对话时，才需要选择或创建伴生 Agent
- 默认关系应在 `Project` 下预先设置，并在 Story 开启会话时作为默认值带出
- 同时允许用户在开启会话时切换为其他模板

### 5. 支持部分模板自动写回共享上下文

- 并非所有模板都只能输出建议
- 部分模板可以具备自动写回能力
- 因此需要在模板层显式声明写回策略、适用对象与风险边界

### 6. 首期优先打通“模板注册 + 使用入口 + 默认继承”

- 不在首期就做完整的重型模板构建页
- 首期重点是让核心业务闭环成立：
  - 模板作为对象存在
  - 用户能在 Project 下看到并使用这些 Agent
  - Story 开启会话时能继承 Project 默认模板

### 7. Story 场景下的 Agent 选择应低存在感

- 不在 Story 创建阶段强提示或强绑定模板
- 只在用户主动开启 Session 时提供模板选择
- 默认值存在，但界面存在感应尽量低，避免打断用户核心流程

### 8. Project 页应支持 `Agent 视图 / Story 视图` 切换

- `Project` 页本身保持轻量
- 一类视图服务“Agent 与共享上下文维护”
- 一类视图服务“Story 推进与组织”
- 不把重型模板构建和运行时调试继续堆到 Project 主页面

## Problem Statement

如果继续沿当前页面与字段模型直接叠加能力，会出现几个问题：

1. **Project 配置职责过重**
   - Project 既要承载基础信息，又要承载默认 Agent 配置、上下文容器、会话编排
   - 再继续加入多模板和工作流后，会迅速变成难以使用的杂糅配置页

2. **模板定义与模板使用没有分层**
   - 模板本身的配置、测试、工作流编排，是重型任务
   - 用户日常与 Agent 协作维护上下文，是轻型任务
   - 这两类行为不应混在同一个入口里

3. **Project 共享上下文对象尚不清晰**
   - 目前上下文更多从 Story / Session 角度建模
   - 还缺少一个稳定的“Project 共享知识/共享上下文”对象与维护入口

4. **Story 默认 Agent 来源不清晰**
   - 某些 Story 下 Agent 其实应来自 Project 级模板
   - 但系统还没有把“哪个模板可作为哪些 Story 场景的默认模板”建模清楚

5. **产品入口与运行时契约混杂**
   - 用户关心的是“我在这个 Project 里有哪些助手、各自负责什么、共享什么上下文”
   - 系统内部关心的是“mount / runtime policy / tool visibility / address space”
   - 如果不重新分层，前端会持续像调试台而不是产品

## Core Question

> 在 AgentDash 中，`Project` 应如何成为“多 Agent 模板与共享上下文的控制面”，同时又不把模板构建、运行时调试、Story 执行默认继承全部混在一个页面里？

## Desired Product Direction

### 1. Project 是 Agent 资产与共享上下文的上层容器

Project 不只是 Story 的上级容器，也应该成为以下对象的归属层：

- Agent 模板库
- Project 共享上下文
- 默认工作流/执行方式
- 可直接对话的 Project Agent 入口

Project 的主要产品职责应收敛为两类：

- `Story 视图`
  - 查看和推进 Story
  - 从项目角度组织工作
- `Agent 视图`
  - 查看当前 Project 下有哪些 Agent
  - 它们分别负责什么
  - 它们维护哪些共享上下文
  - 进入对应会话与 Agent 协作

### 2. 模板构建页与日常使用入口分层

需要明确区分两类入口：

#### A. 重型入口：模板 / Agent 构建页

用于维护：

- Agent 模板定义
- persona / workflow / required context
- 共享上下文挂载规则
- 可暴露的工具/能力边界
- 模板适用场景
- 是否允许作为 Story 默认模板

#### B. 轻型入口：Project 下直接对话

用于：

- 与某个 Project Agent 持续交流
- 补充/整理/沉淀 Project 共享上下文
- 帮助用户准备后续 Story
- 对现有共享上下文做结构化归纳

这个入口应更像“Project Agent Hub”，而不是重型配置台。

### 3. 部分 Project 模板可声明为 Story 默认模板来源

例如：

- `需求整理助手`
- `上下文收集助手`
- `Story 拆解助手`

这些模板不一定直接执行代码，但可以在用户进入 Story 并主动开启会话时，作为默认入口被带出。

默认关系更适合声明成：

- `Project` 为某些 Story 会话场景设置默认模板
- `Story` 开启 Session 时自动带出默认模板
- 用户可在开启时切换

而不是：

- 在 Story 创建流程里强行绑定模板

## Requirements

### 产品对象与边界

- 明确 `Project Agent Template` 是否应成为独立产品对象，而不是继续塞在 `project.config.agent_presets` 里。
- 明确 `Project Shared Context` 是否应成为独立对象或独立子域，而不是只散落在 `context_containers` 中。
- 明确模板的“适用场景”定义，至少能表达：
  - Project 共享上下文维护
  - Story 需求整理
  - Story 上下文收集
  - Story 默认 Agent

补充约束：

- `Project Shared Context` 的用户心智应优先表现为“项目文件/知识资料夹”，而不是 provider 配置集合
- 挂载、权限、runtime 能力属于内部与高级视图
- 用户对话中应能自然引用这些共享上下文内容

### 页面与入口分层

- 明确 `Project 页面` 应保留哪些轻量信息：
  - 当前有哪些 Agent
  - 每个 Agent 负责什么
  - 哪些共享上下文正在被维护
  - 哪些模板可用于创建/协助 Story
- 明确后续独立“模板/Agent 构建页”需要承接哪些重型配置。
- 明确 `Project -> Session` 的直聊入口如何承接：
  - 是 Project 级 companion session
  - 还是 Project 级 agent workspace/session hub

补充约束：

- `Project` 主页需要支持 `Agent 视图 / Story 视图` 切换
- `Project` 主页默认保持轻量，不承担完整模板构建职责
- 首期重点不是完整构建页，而是先打通“看见 Agent、进入使用、默认继承”

### 继承与默认关系

- 明确 `Project 模板 -> Story 默认模板` 的声明方式。
- 明确 Story 创建时如何感知这些默认模板。
- 明确 Story 是否允许覆盖默认模板，以及覆盖后的优先级。

补充约束：

- Story 创建阶段通常不需要感知模板
- 只有在 Story 下主动开启 Session 时，才需要带出默认模板
- 默认模板应来自 Project 预设，并允许用户切换

### 共享上下文维护

- 明确 Project 共享上下文的维护方式：
  - 用户直接编辑
  - 用户通过对话驱动 Agent 整理
  - Agent 写回共享上下文对象
- 明确 Project 共享上下文如何被 Story 复用：
  - 显式引用
  - 默认注入
  - 按模板或场景过滤

补充约束：

- 至少部分模板允许自动写回共享上下文
- 因此需要区分：
  - 只读模板
  - 建议后确认写回模板
  - 可自动写回模板
- 写回行为需要与模板能力边界一起建模

### 架构与运行时承接

- 明确 Project 级 Agent 是否需要独立于 Story / Task 的 owner session 模型。
- 明确 Project 级会话与 Story / Task 会话的边界。
- 明确模板对象、共享上下文对象、运行时会话快照之间的关系，避免继续在前端直接拼接解释链。

补充约束：

- Project 级会话应作为平行于 Story / Task 的正式会话层级
- 用户侧应先看到 Agent 与共享上下文的产品对象
- runtime / permission / mount policy 等内容默认不进入主视图

## Acceptance Criteria

- [ ] 明确 `Project Agent Template` 是否升级为独立产品对象。
- [ ] 明确 `Project Shared Context` 的职责、维护方式与复用方式。
- [ ] 明确 `Project 基础页`、`模板构建页`、`Project 直聊入口` 三者的职责分层。
- [ ] 明确哪些 Project 模板可作为 Story 默认模板，以及声明和覆盖规则。
- [ ] 明确 Project 级会话是否需要独立 session / owner / runtime 抽象。
- [ ] 能拆出后续若干实现任务，而不是继续停留在讨论层。

## Non-Goals

- 本任务不直接实现完整模板编辑器。
- 本任务不直接实现完整 Project Agent runtime。
- 本任务不直接确定最终数据库表结构。
- 本任务不要求立即替换现有全部 `agent_presets` 相关实现。
- 本任务不要求一次性重做全部前端导航结构。

## Proposed Discussion Outputs

希望后续围绕本任务收敛出以下结果：

1. 一版 `Project Agent Template` 产品模型草案
2. 一版 `Project Shared Context` 产品模型草案
3. 一版 `Project 页面 / 模板构建页 / 直聊入口` 的信息架构草案
4. 一版 `Project -> Story 默认模板继承` 规则草案
5. 一版 `Project 级会话` 与 `Story / Task 会话` 的边界定义
6. 若干可执行的拆分任务

## Initial Planning Direction

基于已确认方向，当前推荐的首期规划应优先围绕以下闭环展开：

### Phase 1: 先建立对象与入口

- 引入正式的 `Project Agent Template` 概念
- 在 Project 下形成 `Agent 视图`
- 支持用户看到当前可用的 Project Agent
- 支持进入对应 Project Agent 会话

### Phase 2: 打通共享上下文维护

- 明确 `Project Shared Context` 的用户形态
- 支持 Agent 通过对话整理和维护共享上下文
- 支持部分模板对共享上下文自动写回

### Phase 3: 打通 Story 默认继承

- 在 Project 下声明 Story 会话默认模板
- 在 Story 主动开启 Session 时自动带出默认模板
- 保持默认低存在感，但允许切换

### Phase 4: 再补重型模板构建页

- 模板 persona / workflow / writeback policy / capability boundary
- 高级运行时配置
- 调试、验证与治理能力

## Suggested Follow-up Tasks

1. `project-agent-template-domain-model`
   - 梳理 Project Agent Template 是否独立成正式对象

2. `project-shared-context-product-model`
   - 梳理 Project 共享上下文的对象模型、维护方式与引用方式

3. `project-agent-hub-ui-architecture`
   - 设计 Project 基础页、模板构建页、Project 直聊入口的页面分层

4. `story-default-agent-template-inheritance`
   - 设计 Project 模板到 Story 默认模板的声明、选择与覆盖逻辑

5. `project-session-runtime-boundary`
   - 设计 Project 级 session 与 Story / Task session 的领域边界和运行时契约

## Related Discussion Notes

- Project 级应支持维护多个 Agent 模板及相关工作流
- 模板和 Agent 构建逻辑应由单独页面承接，而不是继续堆叠在 Project 基础页
- 用户后续应支持在 Project 下直接与这些 Agent 对话，以维护 Project 共享上下文
- 部分模板会作为 Story 下 Agent 的默认模板，例如需求整理与上下文收集类 Agent
