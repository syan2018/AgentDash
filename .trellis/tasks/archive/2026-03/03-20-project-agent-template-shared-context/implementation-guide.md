# Project 级 Agent 模板与共享上下文落地手册

## 1. 文档目的

这份文档不是继续讨论“产品应该是什么”，而是用于指导后续实际落地迭代。

目标是把当前项目从已有基础状态推进到以下阶段性目标：

- `Project Agent Template` 成为正式对象
- `Project Agent` 成为用户可见、可用的协作实体
- `Project Shared Context` 成为正式的共享知识对象
- `Project Session` 成为与 `Story / Task Session` 平行的正式会话层级
- `Project` 页面形成 `Story 视图 / Agent 视图`
- Story 会话在启动时支持低存在感地继承 Project 默认模板

这份手册的作用是：

- 确认当前项目真实状态
- 说明首期应该做什么、不应该做什么
- 给出可执行的分阶段实施顺序
- 明确每个阶段应落在哪些层、哪些文件和哪些产品对象上

## 2. 当前项目状态确认

基于当前仓库代码，项目已经具备与本次迭代直接相关的以下基础能力。

### 2.1 已具备的基础

#### A. Project 已经承接了一部分“默认执行配置”

当前 `Project.config` 已包含：

- `default_agent_type`
- `agent_presets`
- `context_containers`
- `mount_policy`
- `session_composition`

对应代码现状：

- 后端 Project 路由已支持 `context_containers / mount_policy / session_composition`
- 前端 Project 页已支持编辑这三类上下文编排配置
- Task / Story 执行路径已经会读取 Project 默认 Agent 和 Project 默认上下文编排

这说明：当前系统已经有“Project 是默认来源层”的基础，但还没有把它提升为完整的产品对象模型。

#### B. Story / Task 侧已具备上下文覆盖能力

当前 Story 已支持：

- `context_containers`
- `disabled_container_ids`
- `mount_policy_override`
- `session_composition_override`

当前 Session 页也已经能看到：

- Project 默认
- Story 覆盖
- 生效后的上下文解释
- Runtime address space

这说明：当前系统已经有“配置合并”和“运行时解释”能力，但仍是以 Story / Task 为中心。

#### C. Session 体系已经存在 owner 概念

当前仓库中的 Session owner 主要是：

- `story`
- `task`

相关逻辑已经散落在：

- 后端 session binding / acp session 路由
- Story session 路由
- Task session 路由
- 前端 SessionPage 的 owner 分支处理

这说明：系统不是没有 owner 分层，而是还没有把 `project` 作为正式 owner 加进去。

### 2.2 当前明显缺失的部分

#### A. `Project Agent Template` 仍未成为正式对象

现在仍主要依赖：

- `project.config.agent_presets`
- `project.config.default_agent_type`

这属于“执行配置字段”，不等于正式的模板对象。

缺失的内容包括：

- 模板对象的稳定标识
- 模板用途/适用场景
- 写回策略
- 对共享上下文的访问边界
- 是否可作为 Story 默认模板

#### B. `Project Agent` 仍未成为用户可见实体

当前用户在 Project 维度还看不到：

- 这个 Project 下到底有哪些 Agent
- 每个 Agent 的职责是什么
- 每个 Agent 维护哪些共享上下文
- 从 Project 直接进入 Agent 会话的入口

也就是说，现在只有模板配置的影子，没有真正的“Agent 视图”。

#### C. `Project Shared Context` 还没有正式产品表达

虽然当前有 `context_containers` 和 Address Space，但对用户来说仍然过于底层。

目前缺少：

- 一个稳定的 Project 共享知识对象
- 面向用户的目录/资料表达
- Agent 与共享知识之间的产品级关联

#### D. 还没有 `Project Session`

当前 owner 只有 `story / task`，没有 `project`。

这会直接导致：

- 无法在领域模型上承接 Project 级 Agent 对话
- 前端 Session 页也无法自然回到 Project Agent 视图
- Project 直聊入口只能暂时伪装成 Story/Task 变体

#### E. Project 页面还不是产品控制面

当前 Project 页主要还是：

- 基础信息
- 项目配置
- 上下文编排配置
- 工作空间

还没有形成：

- `Story 视图`
- `Agent 视图`

也没有 `Project Agent Hub` 入口。

## 3. 本次迭代的目标边界

本次迭代不是要“一次性做完整体系”，而是要正式开始进入这条迭代线。

因此，本次开始迭代后的目标应收敛为：

### 3.1 首期必须建立的闭环

1. 领域上正式承认以下对象
   - `Project Agent Template`
   - `Project Agent`
   - `Project Shared Context`
   - `Project Session`

2. 产品上形成以下入口
   - `Project -> Story 视图`
   - `Project -> Agent 视图`
   - `Project Agent -> Session`

3. 交互上打通以下链路
   - 在 Project 下看到可用 Agent
   - 与 Project Agent 对话
   - 维护共享上下文
   - Story 开启 Session 时带出默认模板

### 3.2 首期明确不做的内容

- 完整的模板构建工作台
- 全量 runtime 调试面板重构
- 一次性替换所有旧字段
- 一次性重做整个导航

## 4. 推荐的落地顺序

推荐按“先对象，再入口，再默认继承，再治理”的顺序推进。

### Phase 1: 建立正式领域对象

这一阶段的目标是停止继续围绕 `project.config.agent_presets` 直接堆功能。

#### 建议产出

- 新的 `Project Agent Template` 领域对象草案
- 新的 `Project Agent` 领域对象草案
- 新的 `Project Session` owner 类型
- 新的 `Project Shared Context` 基础模型

#### 后端优先事项

- 先梳理 Domain 层对象，不急着一次性替换数据存储
- 允许通过适配层把旧 `agent_presets` 映射成新模板对象，作为过渡
- 先让 API 具备读取 Project Agent 列表的能力

#### 前端优先事项

- 不先做复杂编辑器
- 先保证 `Project` 页有能力展示 Agent 列表和入口

### Phase 2: 打通 Project Agent Hub

这一阶段的目标是让“Project 下直接与 Agent 对话”成为正式能力。

#### 建议产出

- `Project Session` API
- `Project Agent Hub` 页面或抽屉
- Project 级会话跳转与返回路径

#### 后端优先事项

- 扩展 `SessionOwnerType`，加入 `project`
- 补齐 Project session binding / list / detail / create / unbind 路由
- 明确 Project session 的上下文构建逻辑

#### 前端优先事项

- Session 页增加对 `project` owner 的识别
- Project Agent 入口支持打开对应 session
- 明确从 Session 返回 `Project Agent 视图` 的路径

### Phase 3: 打通 Project Shared Context

这一阶段的目标是让 Project 共享上下文不再只是底层 container，而是一个正式产品对象。

#### 建议产出

- 一版用户可理解的默认目录模型
- 一版 Project Shared Context 读取/写回接口
- 一版 Project Agent 与 Shared Context 的关联方式

#### 产品表达原则

默认表达应是：

- 文件夹
- 资料
- 知识目录

而不是：

- provider
- mount policy
- capability matrix

#### 后端优先事项

- Shared Context 模型先可轻量落在现有 container 能力之上
- 先建立“产品对象 -> 运行时映射”桥接，而不是反过来让前端直接消费 runtime 结构

#### 前端优先事项

- Agent 视图中显示“该 Agent 维护哪些共享资料”
- 对话中支持更自然地传递/引用共享上下文

### Phase 4: 打通 Story 会话默认继承

这一阶段的目标是让 Story 在开启 Session 时，低存在感继承 Project 默认模板。

#### 建议产出

- `Story Session Default Template Binding`
- Story 开启 Session 时的默认模板解析逻辑
- Session 启动时可切换模板，但默认低存在感

#### 关键原则

- Story 创建时不强绑定模板
- 只有用户主动开启会话时才带出默认模板
- 用户可以换，但不需要被强提示教育“模板是什么”

### Phase 5: 完善模板治理与构建页

在前四阶段闭环成立后，再进入重型能力。

#### 建议产出

- 模板构建页
- 写回治理配置
- 权限与能力边界配置
- 调试与验证面板

## 5. 建议的实现切片

为了让项目能真正开始迭代，建议把下一步拆成几个非常明确的切片。

### Slice A: 领域模型切片

目标：

- 把 `Template / Agent / Project Session / Shared Context` 正式建模

建议文件方向：

- Domain 层新增对象定义
- Application 层新增读取/组装逻辑
- 不急着一次性替换现有 ProjectConfig

### Slice B: Session owner 扩展切片

目标：

- 支持 `project` owner

建议修改范围：

- `SessionOwnerType`
- Session binding 相关 API
- Session 页面返回逻辑
- ACP session 路由 owner 分支

### Slice C: Project Agent Hub UI 切片

目标：

- 在 Project 页面建立 `Agent 视图`

建议修改范围：

- Project 页面结构
- Agent 卡片 / 列表
- 打开 session 的入口

### Slice D: Shared Context 产品表达切片

目标：

- 定义 Project Shared Context 的首期用户表达

建议修改范围：

- 类型定义
- API 汇总对象
- 前端显示层

### Slice E: Story 默认模板切片

目标：

- 在 Story 主动开启会话时继承默认模板

建议修改范围：

- Story session 启动入口
- Task/Story Agent 选择入口
- Project 默认模板设置读取

## 6. 本轮开始迭代的执行建议

正式开始这条迭代时，不建议一上来就写页面，而应先做这三件事：

### 6.1 先把领域对象和 owner 边界定住

优先完成：

- `Project Agent Template`
- `Project Agent`
- `Project Session`
- `Project Shared Context`

这一步定不住，后面前端会继续在旧字段上做补丁。

### 6.2 再打通最短使用链路

最短链路应该是：

`Project Agent 视图 -> 打开 Project Session -> 对话 -> 维护共享上下文`

只要这条链先成立，产品方向就真正开始落地了。

### 6.3 最后再把 Story 默认继承接上

当 Project Agent 与 Project Session 已经存在后，再做：

`Story 主动开启 Session -> 自动带出默认模板`

这样能避免把 Story 创建流程和默认模板逻辑过早耦合。

## 7. 风险提醒

### 7.1 最大风险：继续在旧字段上叠功能

如果继续直接围绕：

- `default_agent_type`
- `agent_presets`
- `context_containers`

追加页面逻辑，而不先抽正式对象，最终会得到一个更难迁移的配置系统。

### 7.2 第二风险：过早做重型构建页

如果在 `Project Agent Hub` 和 `Project Session` 还没站稳之前就先做复杂模板编辑器，会导致：

- 用户还没有真正的日常使用入口
- 产品先变成治理台而不是协作产品

### 7.3 第三风险：让运行时结构继续泄漏到用户主界面

当前项目已经出现这个趋势：

- runtime
- mount
- tool visibility
- address space

这些默认不应成为用户产品表面的核心语言。

## 8. 完成定义

可以认为这条迭代“正式开始”的标志不是写了多少页面，而是同时满足以下条件：

- 已有正式的任务与落地文档
- 当前对象边界已经明确
- 下一步切片已经明确
- 当前 task 已被切为进行中

当以下能力成立时，可以认为首期闭环完成：

- Project 下能看到 Agent 视图
- 能从 Project 直接与 Agent 对话
- Project Shared Context 开始成为正式对象
- Story 开启会话时能低存在感继承 Project 默认模板

## 9. 建议的下一步任务

建议紧接着从以下顺序开始拆子任务：

1. `project-session-owner-model`
2. `project-agent-template-domain-bridge`
3. `project-agent-hub-ui`
4. `project-shared-context-first-model`
5. `story-session-default-template-binding`
