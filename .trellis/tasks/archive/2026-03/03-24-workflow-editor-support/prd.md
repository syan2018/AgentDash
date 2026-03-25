# 前端正式支持 Workflow 编辑

## Goal

为 AgentDashboard 提供一套**正式可用**的 Workflow 制定与编辑能力，使 Workflow 不再主要依赖后端内置 JSON 模板和 bootstrap 流程，而是可以由前端作为主要入口完成：

- 创建 / 编辑 Workflow Definition
- 编辑 Phase、Binding、完成条件和记录策略
- 在保存前执行服务端语义校验
- 在 UI 中预览运行时投影效果
- 将编辑后的 Workflow 分配到 Project / Role 并直接投入运行

目标不是只做一个“能编辑 JSON 的页面”，而是交付一套**前端可制定、后端可校验、运行时可消费**的完整能力闭环。

## Why This Exists

当前 Workflow 模块已经具备基础运行时能力，但“制定”和“编辑”仍然没有被正式产品化：

- 前端目前只能：
  - 查看 workflow templates / definitions
  - bootstrap 内置模板
  - 为 project 分配 workflow
  - 启动 / 激活 / 完成 run
- 后端当前缺少完整的 definition 创建 / 更新 / 校验 / 预览 API
- Binding / locator 仍然偏字符串约定，前端无法安全构建正式编辑器
- `recommended_role` 目前只存在于 builtin template，不是 definition 的正式字段
- 运行时投影、completion 判定、binding 解析都依赖后端真实语义，前端无法独立保证编辑结果可运行

如果继续维持现状，Workflow 会停留在“后端内置流程演示能力”，无法成为平台内正式的可配置系统。

## Product Principles

### P1: 前端是制定入口，后端是语义真源

- Workflow 的创建和编辑由前端完成
- Workflow 的合法性、可运行性、预览结果由后端负责最终裁定
- 不允许前端绕过后端语义校验直接写入不可信 definition

### P2: 编辑器必须是结构化编辑，不是裸 JSON 编辑

- 用户应通过结构化表单 / builder 编辑 workflow
- Binding、completion mode、artifact type、role 等关键字段应优先通过枚举 / registry 选择
- 仅在必要场景提供高级文本输入，不把系统语义暴露成一堆不可控字符串

### P3: Workflow Definition 必须携带完整产品语义

- Definition 不仅要能描述 phase 列表
- 还要正式承载：
  - 适用 target_kind
  - 推荐 / 适用 role
  - record policy
  - 编辑来源 / 状态
  - phase 级完成语义与上下文语义

### P4: 预览能力必须和运行时一致

- 编辑器中的“预览”必须走与真实 runtime 相同或等价的后端解析链路
- 不能出现“编辑器看起来能用，实际 run 时无法解析 binding / 无法推进 phase”的情况

### P5: 当前项目不做兼容性包袱设计

- 这是预研期项目，不需要为历史 schema / API 做额外兼容方案
- 如现有模型不适合正式编辑，应直接调整到更正确的状态

## Current State Assessment

### 已有基础

- Domain 已有 `WorkflowDefinition / WorkflowAssignment / WorkflowRun`
- Run 生命周期已支持 `start / activate / complete / append artifacts`
- Completion 已支持 `manual / session_ended / checklist_passed`
- Projection / binding 解析链路已经存在
- 前端已有 workflow 管理和任务执行面板

### 当前主要缺口

- 没有正式的 definition create / update / delete / validate / preview API
- 没有 editor-friendly 的 binding registry / metadata API
- Definition 缺少 role 归属等正式字段
- 前端只有“模板注册 + 分配 + 推进 run”，没有“定义 workflow”的能力
- 缺少围绕编辑态的服务端语义校验与预览反馈

## Scope

### In Scope

- Workflow Definition 的完整创建 / 编辑 / 保存 / 停用能力
- Workflow Phase 的增删改排
- Binding 的结构化编辑
- Record Policy 的编辑
- Completion Mode 的结构化配置
- 服务端 definition 校验 API
- 服务端 phase/runtime preview API
- Workflow 角色归属模型的正式化
- 前端 Workflow Editor 页面 / 交互流
- 与 assignment / run 链路的集成验证

### Out of Scope

- 动态加载外部 workflow plugin
- 可视化拖拽画布式 BPMN 设计器
- 多人实时协同编辑
- 完整版本历史 / 回滚中心
- 通用脚本执行 DSL

## Requirements

### R1: 提供正式的 Workflow Definition 写接口

后端必须提供 definition 的正式写接口，至少覆盖：

- 创建 definition
- 更新 definition
- 查询 definition 详情
- 停用 / 启用 definition
- 删除 definition（如产品策略允许）

接口必须返回结构化错误，而不是仅返回泛化字符串失败。

### R2: Definition 模型必须补齐编辑所需元数据

`WorkflowDefinition` 或等价正式模型必须显式承载以下能力，而不是继续依赖 builtin template 侧推断：

- 推荐角色或适用角色
- 编辑来源（builtin seed / user-authored / cloned）
- definition 状态（draft / active / disabled 等，如采用此模型）
- 可供前端展示的稳定 metadata

### R3: 提供服务端语义校验能力

需要独立的 validate 能力，用于在保存前或编辑中校验 definition：

- phase 基础字段是否合法
- phase 排序是否合法
- completion mode 与 binding 组合是否合法
- requires_session 与 target_kind / phase 语义是否匹配
- artifact 配置是否完整
- binding locator 是否属于合法集合

校验结果必须是结构化返回，至少包含：

- 错误码
- 人类可读说明
- 对应字段路径
- 严重级别（error / warning）

### R4: 提供 Binding Registry / Metadata 能力

后端必须提供 editor 可消费的 binding 元数据来源，至少覆盖：

- 可选 binding kind
- 各 kind 的合法 locator 列表或构造规则
- 每种 binding 的说明、适用 target_kind、是否 required、示例内容
- ActionRef / Checklist / RuntimeContext 的可选项说明

前端不应继续把 locator 当作纯自由文本核心输入。

### R5: 提供 Workflow Preview / Simulation 能力

后端必须提供“definition + sample target context”的预览能力，至少支持：

- 展示 phase 列表及其解析后的摘要
- 解析当前 phase bindings 的结果
- 预览 agent_instructions 注入效果
- 标记 unresolved / missing bindings
- 给出 completion mode 的说明和潜在阻塞点

该能力应复用现有 projection / binding 解析链路，而不是另写一套假逻辑。

### R6: 前端提供正式 Workflow Editor

前端应新增 workflow editor 交互，至少包含：

- definition 基本信息编辑
- phase 列表编辑与重排
- phase 详情编辑
- binding 添加 / 删除 / 配置
- completion mode 与 artifact 配置编辑
- record policy 编辑
- validate / preview / save / disable 等操作

UI 应区分：

- 草稿未保存
- 校验中
- 校验失败
- 可保存
- 已保存但未分配
- 已分配且可运行

### R7: Workflow 与 Role 的分配关系必须清晰

产品和数据模型必须明确：

- 一个 definition 是否可以服务多个 role
- role 归属是 definition 自带 metadata，还是 assignment 决定
- 项目页如何分组展示自定义 workflow

如果 definition 存在推荐角色，应作为正式字段，而不是仅从 builtin template 推断。

### R8: 编辑结果必须能无缝进入运行链路

编辑并保存后的 workflow，必须能直接完成：

- project assignment
- run start
- phase activation
- artifact append
- completion / auto progression

不允许出现“可编辑但无法运行”的中间态能力。

### R9: 前后端错误模型必须一致

前端对 workflow 编辑相关错误必须有清晰映射：

- 字段级错误
- phase 级错误
- binding 级错误
- 保存冲突 / 停用态限制
- preview 失败

不允许所有失败都退化成 toast + message string。

### R10: 需要补齐测试与文档

至少补齐：

- domain 校验测试
- application validate / preview / save 测试
- API 路由测试
- frontend editor 关键交互测试
- workflow 编辑与运行的端到端主链路测试

并同步更新与 workflow 模块相关的设计 / 研发文档。

## Proposed Design Direction

### 1. 后端模型层

- 将 definition 升级为正式可编辑资源
- 为 definition 补齐 editor-facing metadata
- 视需要收敛 builtin template 与 user-authored definition 的关系：
  - builtin template 作为 seed
  - definition 作为真正运行和编辑对象

### 2. 后端应用层

- 在现有 `WorkflowCatalogService` 之上扩展 definition create / update / validate / preview 能力
- 对 preview 复用已有 `projection` / `binding` 逻辑
- 对 validate 形成结构化结果模型，避免只有字符串错误

### 3. API 层

建议新增一组正式接口，至少包括：

- `POST /workflows`
- `PUT /workflows/{id}`
- `GET /workflows/{id}`
- `POST /workflows/validate`
- `POST /workflows/preview`
- `POST /workflows/{id}/enable`
- `POST /workflows/{id}/disable`

如产品策略允许，再考虑 delete。

### 4. 前端层

- 在现有 project workflow panel 之外，新增正式 editor 页面或抽屉
- 将 workflow service / store 从“运行态操作”为主，扩展到“定义编辑态”为主
- 建立 definition draft state、validation result state、preview state

### 5. 运行时层

- 保持 run / projection / completion 为运行时真源
- editor preview 只允许复用运行时语义，不允许另写平行语义层

## Acceptance Criteria

- [ ] 可以在前端新建一个自定义 Workflow Definition
- [ ] 可以在前端编辑 definition 基本信息、phase 列表、bindings、completion mode、record policy
- [ ] definition 保存前可调用服务端 validate，并拿到字段级错误
- [ ] definition 可通过服务端 preview 查看 phase 解析结果与 binding 状态
- [ ] 自定义 workflow 有正式的 role 归属或推荐角色语义，前端无需再依赖 builtin template 推断
- [ ] 已保存 definition 可以在项目页完成 assignment
- [ ] assignment 后可在 Task / Story / Project 上正常启动 run
- [ ] run 能正常经历 activate / complete / append artifacts / auto completion 主链路
- [ ] 非法 workflow 配置会在 validate/save 阶段被清晰拦截
- [ ] 前后端都具备对应测试，覆盖主要成功 / 失败路径

## Good / Base / Bad Cases

### Good

- 用户创建一个 Task workflow，添加 `start / implement / check / record`
- `check` phase 使用合法 checklist binding 和 `checklist_passed`
- preview 能解析所有 bindings
- 保存、分配、启动 run、完成 phase 全链路成功

### Base

- 用户基于 builtin seed 克隆一个 workflow
- 只修改 phase 标题、instructions、record policy
- validate 无错误
- 保存后可正常分配并运行

### Bad

- `checklist_passed` phase 没有 checklist evidence 相关 binding
- `requires_session=true` 但 phase 语义和 target 不成立
- locator 使用不存在的 runtime context key
- preview 返回 unresolved binding
- save 被后端拒绝并附带字段级错误路径

## Technical Notes

- 当前项目处于预研期，不需要为旧 schema / 旧 API 设计兼容层；必要时可以直接调整模型
- 本任务是 fullstack 任务，至少会涉及 domain / application / api / frontend / runtime preview 链路
- 运行时 preview 必须尽可能复用现有 workflow projection 逻辑，避免“编辑器语义”和“真实执行语义”分叉
- 若 workflow role 归属模型与现有 assignment 模型冲突，应优先收敛到更清晰的正式模型

## Initial Files Likely To Change

- `crates/agentdash-domain/src/workflow/entity.rs`
- `crates/agentdash-domain/src/workflow/value_objects.rs`
- `crates/agentdash-application/src/workflow/catalog.rs`
- `crates/agentdash-application/src/workflow/projection.rs`
- `crates/agentdash-application/src/workflow/binding.rs`
- `crates/agentdash-api/src/routes/workflows.rs`
- `crates/agentdash-api/src/dto/workflow.rs`
- `frontend/src/services/workflow.ts`
- `frontend/src/stores/workflowStore.ts`
- `frontend/src/features/workflow/project-workflow-panel.tsx`
- `frontend/src/features/workflow/`

## Deliverables

- 一套正式 workflow definition 编辑 API
- 一套结构化 validate / preview 返回模型
- 一套前端 workflow editor UI
- 一组围绕 workflow 编辑与运行联动的测试
- 一份更新后的 workflow 编辑设计说明 / 研发说明
