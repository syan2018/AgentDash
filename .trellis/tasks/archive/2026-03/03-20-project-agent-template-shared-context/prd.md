# Project 级 Agent 模板与共享上下文规划

## 1. 任务目标

本任务用于把 `Project` 从“Story 的上级容器”进一步明确为：

- Agent 资产的管理层
- 项目共享上下文的沉淀层
- Project 级协作会话的承载层
- Story 会话默认模板的来源层

目标不是立即实现所有页面和运行时能力，而是先形成一版可持续推进的完整规划，避免后续继续围绕零散字段和临时页面做加法。

本规划希望回答的核心问题是：

> AgentDash 应如何在 `Project` 层同时承载多 Agent 模板、共享上下文维护、Project 直聊入口，以及 Story 会话默认模板继承，而又不把重型配置、日常使用和运行时调试混在一起？

## 2. 产品北极星

对用户来说，一个 `Project` 应该同时有两种自然使用方式：

### 2.1 Story 视角

用户从项目角度组织需求、查看 Story、推进执行。

### 2.2 Agent 视角

用户从项目角度查看当前有哪些 Agent、它们分别负责什么、维护哪些共享上下文，并可以直接与它们对话。

这两种视角都属于 `Project` 的正常使用方式，但面向的是不同心智：

- `Story 视角` 解决“我要推进什么工作”
- `Agent 视角` 解决“我可以找谁协作、项目知识沉淀在哪里”

因此，`Project` 主页应是一个轻量控制面，而不是把模板构建、权限编排、运行时调试全部堆在一起的配置台。

## 3. 设计原则

### 3.1 模板与 Agent 必须分离

- `Template` 是定义
- `Agent` 是可被用户直接使用的实体

用户在 `Project` 下看到的应该是“有哪些 Agent 可协作”，而不是“有哪些模板 JSON 已配置”。

### 3.2 用户默认看到的是“文件/资料”，不是“挂载实现”

Project 共享上下文对用户的默认表现应接近原生文件系统：

- 多个 mount 更像多个文件夹
- 用户在对话中应能自然引用这些内容
- provider、permission、capability、runtime policy 默认属于高级/内部视图

### 3.3 重型配置与轻型使用必须分层

应明确区分：

- 重型入口：模板/Agent 构建页
- 轻型入口：Project 下直接与 Agent 对话

前者服务于编排和治理，后者服务于日常协作。

### 3.4 Story 创建不强绑定模板

Story 创建通常是独立行为，不应在创建阶段强行引入 Agent 模板。

模板只在以下时机低存在感介入：

- 用户在 Story 下主动开启 Session
- 系统从 Project 默认设置中带出建议模板
- 用户可在开启会话时切换

### 3.5 Project 级会话必须成为正式层级

Project 下直接与 Agent 对话，不应只是 Story / Task session 的变体。

需要一个与 Story / Task 平行的正式 `Project Session` 层，用于：

- 沉淀项目知识
- 维护共享上下文
- 协助准备后续 Story

### 3.6 写回能力必须显式治理

部分模板允许自动写回共享上下文，但这不是默认行为。

写回能力必须作为模板定义的一部分被显式声明，而不是依赖实现细节或前端临时判断。

## 4. 目标对象模型

本规划建议把当前相关能力整理为以下正式对象。

### 4.1 Project

Project 是顶层容器，负责聚合：

- Story 集合
- Project Agent Template 集合
- Project Agent 集合
- Project Shared Context
- Project Session 集合
- Story 会话默认模板设置

Project 不再承担所有重型配置细节本身，而是作为这些对象的归属层和入口层。

### 4.2 Project Agent Template

`Project Agent Template` 是独立正式对象，用于描述一个 Agent 的定义，而不是直接等同于可使用实例。

它至少应包含以下职责：

- Agent 身份与定位
- persona / workflow / required context
- 共享上下文访问范围
- 工具/权限能力边界
- 写回策略
- 适用场景
- 是否可作为 Story 会话默认模板

它的目标不是被用户频繁日常编辑，而是由较重的“模板构建页”承接配置和治理。

### 4.3 Project Agent

`Project Agent` 是用户在 Project 中实际可见和可协作的实体。

它由某个模板启用而来，重点面向使用而非定义。

用户在 `Agent 视图` 下应看到的是：

- Agent 名称
- Agent 负责的职责
- Agent 维护的共享上下文范围
- 最近会话或状态摘要
- 进入对话入口

### 4.4 Project Shared Context

`Project Shared Context` 是独立产品对象，用于承载整个 Project 维度的共享知识与可复用上下文。

对用户的主心智应是“项目资料文件夹/知识资料夹”，而不是底层容器集合。

推荐的默认结构可以从如下目录心智开始：

- `/project/overview`
- `/project/domain`
- `/project/decisions`
- `/project/references`
- `/project/story-prep`

这不要求首期一次性做成完整文件系统，但产品表达应围绕这个心智组织。

### 4.5 Project Session

`Project Session` 是 Project 级正式会话对象，与 Story / Task session 平行。

它的典型用途包括：

- 与 Project Agent 对话
- 维护共享上下文
- 整理项目背景和约束
- 为未来 Story 做准备

### 4.6 Story Session Profile / Default Template Binding

Project 需要维护一套“Story 会话默认模板设置”，用于在 Story 主动开启 Session 时自动带出默认模板。

这里表达的不是“创建 Story 时就绑定模板”，而是：

- Project 预设某类 Story 会话的默认模板
- Story 开启 Session 时自动带出
- 用户可在开启时切换

## 5. 用户心智模型

为了避免产品继续滑向“调试台”，需要明确用户默认看到的是什么。

### 5.1 用户默认看到

- 这个 Project 下有哪些 Agent
- 每个 Agent 负责什么
- Project 里有哪些共享资料/知识
- Story 会话默认会由哪个 Agent 协助

### 5.2 用户默认不需要看到

- provider 类型
- mount derivation policy
- runtime permission policy
- address space 细节
- tool visibility 细节

这些内容应进入高级配置或调试视图，而不是主产品表面。

## 6. 页面与信息架构

### 6.1 Project 页面

Project 页面应保持轻量，并支持两种主视图切换：

#### A. Story 视图

面向工作推进：

- Story 列表
- Story 状态
- 进入 Story 详情

#### B. Agent 视图

面向 Agent 协作：

- Project 下有哪些 Agent
- 每个 Agent 的定位与职责
- 与哪些共享上下文相关
- 进入 Project Agent 会话

Project 页面不应继续承担完整模板构建职责。

### 6.2 模板/Agent 构建页

这是独立重型页面，用于承接真正复杂的定义能力，包括：

- persona / workflow / required context
- 工具/权限边界
- 写回策略
- 共享上下文访问规则
- 适用场景
- 是否可作为 Story 默认模板

首期不要求完整落地，但必须作为明确的后续页面边界写入规划。

### 6.3 Project Agent Hub

Project 级对话入口应更像一个 `Agent Hub`：

- 选择或查看某个 Project Agent
- 进入该 Agent 的会话
- 在会话中维护共享上下文
- 将结果沉淀回 Project Shared Context

它是轻型协作入口，不是模板治理页。

### 6.4 Story 中的 Agent 入口

Story 页面不需要在创建阶段强化模板概念。

仅在以下时机呈现模板选择：

- 用户主动开启 Session
- 系统从 Project 默认设置中带出推荐模板
- 用户可以切换，但默认存在感应尽量低

## 7. 继承与默认关系

### 7.1 继承原则

Project 是默认来源层，Story 是局部覆盖层。

但在模板继承问题上，默认关系应限制在“会话启动时”生效，而不是提前污染 Story 创建流程。

### 7.2 建议继承链

1. Project 维护一组 Story 会话默认模板设置
2. 用户创建 Story 时，不强制绑定模板
3. 用户在 Story 下主动开启 Session 时：
   - 系统自动带出默认模板
   - 用户可切换为其他模板
4. 一旦会话已经启动，会话使用自己的模板解析结果

### 7.3 产品目标

这样可以同时满足：

- 默认值存在
- 存在感低
- 不打断 Story 创建
- 用户仍有切换自由

## 8. 共享上下文维护模型

Project Shared Context 需要同时支持“用户维护”和“Agent 参与维护”。

### 8.1 维护方式

- 用户直接编辑
- 用户通过对话驱动 Agent 整理
- Agent 结构化写回共享上下文

### 8.2 复用方式

Project Shared Context 后续应能被 Story 复用，至少支持：

- 显式引用
- 作为默认共享背景被引入
- 按模板或场景过滤

### 8.3 用户表达

无论内部是否仍使用 context container / mount / address space，这一层对用户的表达都应优先是：

- 文件夹
- 资料
- 知识沉淀
- 可复用项目背景

## 9. 写回治理策略

模板需要显式声明写回策略，建议收敛为三级：

- `read_only`
- `confirm_before_write`
- `auto_write`

默认策略应为 `confirm_before_write`，只有少数明确授权模板可使用 `auto_write`。

### 9.1 read_only

- 只能读取和分析
- 不负责正式改写共享上下文

### 9.2 confirm_before_write

- Agent 可提出结构化变更
- 用户确认后写回
- 适合作为默认治理模式

### 9.3 auto_write

- Agent 可自动将结果写回共享上下文
- 需要更明确的模板职责、写入范围和风险边界

## 10. 运行时边界

本规划不否认底层仍然需要：

- mount
- provider
- permission policy
- tool visibility
- address space

但这些属于运行时与调试层，不应成为用户主界面的核心对象。

建议的边界是：

- 用户主视图：Agent、资料、会话、默认关系
- 高级配置视图：模板编排与权限能力
- 调试视图：运行时快照与工具可见性

## 11. 首期实施方向

首期不应试图一次性做完整体系，而应优先建立闭环。

### Phase 1: 建立正式对象与轻量入口

- 引入 `Project Agent Template`
- 引入 `Project Agent`
- Project 页面支持 `Story 视图 / Agent 视图`
- 用户可从 Project 进入 Agent 会话

### Phase 2: 打通共享上下文维护

- 引入 `Project Shared Context` 的用户表达
- 支持 Agent 通过会话维护共享上下文
- 落地三级写回策略中的基础能力

### Phase 3: 打通 Story 会话默认继承

- Project 可设置 Story 会话默认模板
- Story 主动开启 Session 时自动带出默认模板
- 用户可切换，且默认存在感低

### Phase 4: 补重型模板构建页

- 完整模板编辑
- persona / workflow / capability / writeback policy
- 运行时调试与治理能力

## 12. 不做什么

本任务不包含以下范围：

- 立即实现完整模板编辑器
- 立即重做所有数据库结构
- 一次性替换现有全部 `agent_presets`
- 一次性重做全部前端导航
- 把运行时调试信息直接当成用户主界面

## 13. 验收标准

- [ ] 形成一套系统化的 `Project / Template / Agent / Shared Context / Session` 规划结构
- [ ] 明确模板与 Agent 的分层关系
- [ ] 明确 Project 页面、模板构建页、Project Agent Hub 的页面边界
- [ ] 明确 Story 会话默认模板的继承时机和切换方式
- [ ] 明确 Project Shared Context 的用户心智与写回治理策略
- [ ] 能继续拆分为若干具体实施任务

## 14. 建议拆分任务

1. `project-agent-template-domain-model`
   - 梳理 Template / Agent / Session / Shared Context 的正式领域模型

2. `project-agent-hub-information-architecture`
   - 设计 Project 页面中的 `Story 视图 / Agent 视图` 与 Project Agent Hub

3. `project-shared-context-product-model`
   - 明确共享上下文的用户表达、目录组织和引用方式

4. `story-session-default-template-binding`
   - 设计 Project 到 Story Session 的默认模板继承与切换机制

5. `template-writeback-governance`
   - 设计 `read_only / confirm_before_write / auto_write` 治理模型

6. `agent-template-builder-scope`
   - 明确重型模板构建页的首期边界与后续能力地图
