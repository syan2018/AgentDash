# 解耦 API 与 Domain 业务重构

## Goal

规划 `agentdash-api` 当前对领域业务过度承载的问题，明确后续如何把 transport、application orchestration、domain rule、context composition、session execution 等职责重新拉开边界，形成更稳定的后端分层演进路线。

这个任务的目标不是一口气重写整个后端，而是先明确“哪里耦合过重、应该迁到哪里、按什么顺序迁”，以便后续重构能持续推进，而不是每次只在 `agentdash-api` 中继续堆业务。

## Background

随着最近几轮功能推进，`agentdash-api` 已经承接了大量不只是“HTTP / WebSocket 入口”的职责，例如：

- Task 启动、续跑、取消、session 绑定与状态协调
- context composer / session plan 拼装
- Story / Task / Project 级上下文与虚拟容器的聚合
- runtime tools / MCP / address space 的组装与注入
- 多层 repository 读取后的业务判断与状态推进

这些能力本身没有问题，但它们目前大量集中在 `agentdash-api`：

- `routes` 里直接做业务判断和领域对象改写
- `bootstrap` 中承接大量跨实体编排
- `task_agent_context`、`session_plan` 等逻辑仍直接挂在 API crate
- `api` crate 同时知道 transport、application、domain、部分 runtime 组装细节

这会带来两个长期问题：

1. 新业务越来越容易继续堆到 `agentdash-api`
2. 领域逻辑和 transport 逻辑混在一起后，很难形成稳定的演进边界

## Problem Statement

当前的主要耦合并不是“引用 domain crate”本身，而是以下几类更实质的问题：

### 1. Route Handler 过厚

部分 route 已经不只是：

- 解析输入
- 调用 use-case
- 映射响应

而是在 handler 内直接：

- 读取多个 repository
- 做跨实体校验
- 拼装领域对象
- 处理状态推进
- 决定错误语义

### 2. API Crate 内聚合了过多业务编排

例如：

- Task execution gateway
- session plan
- task agent context
- address space 组装

这些逻辑更接近 application/use-case 编排层，而不是 transport 层。

### 3. Domain 对象直接承担过多“出站展示结构”

当前不少 API 返回值仍直接暴露 domain 实体或与其强耦合，这会让：

- API 语义难以独立演进
- 前端模型跟随 domain 细节波动
- transport 层更难做稳定 DTO / view model

### 4. Context / Session / Runtime 相关逻辑边界不清

当前以下几块逻辑交织较深：

- session plan
- context contributor / context snapshot
- task execution
- MCP 注入
- address space summary

这些逻辑本身都是“用例级编排”，但目前散落在 API crate 内部。

## Requirements

- 盘点 `agentdash-api` 中当前最重的业务耦合热点，形成结构化清单。
- 定义目标分层：
  - transport layer
  - application / use-case orchestration
  - domain
  - infrastructure / runtime adapter
- 明确哪些逻辑应从 `agentdash-api` 迁移到 `agentdash-application`。
- 明确哪些逻辑应保留在 API 层，仅作为：
  - 请求解析
  - 鉴权
  - 错误映射
  - 响应 DTO 输出
- 明确 `session_plan / task_agent_context / task_execution_gateway / address_space_access` 各自的归属建议。
- 明确 API 返回是否需要引入更稳定的 response DTO / assembler 层，减少 domain 实体直接外露。
- 明确重构顺序，避免一次性大迁移导致功能回归。

## Acceptance Criteria

- [ ] 明确 `agentdash-api` 当前的主要业务耦合热点与代表文件。
- [ ] 明确目标分层及各层职责边界。
- [ ] 明确至少 3 组应迁移到 `agentdash-application` 的核心能力。
- [ ] 明确 routes 应收缩到什么程度。
- [ ] 明确 DTO / assembler / mapper 是否应成为独立层。
- [ ] 给出可分阶段执行的重构顺序，而不是笼统“以后再拆”。

## Hotspot Candidates

建议优先评估以下文件与模块：

- `crates/agentdash-api/src/routes`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `crates/agentdash-api/src/task_agent_context.rs`
- `crates/agentdash-api/src/session_plan.rs`
- `crates/agentdash-api/src/address_space_access.rs`
- `crates/agentdash-api/src/routes/acp_sessions.rs`

这些位置共同特点是：

- 已经超出单纯 API 接入职责
- 对 domain、executor、mcp、relay、repository 有跨层认知
- 后续新需求也最容易继续往里长

## Proposed Target Boundary

### 1. API 层

只保留：

- HTTP / WS 协议适配
- 请求参数解析
- 身份与权限入口
- 统一错误映射
- response DTO 输出

API 层不再直接承担复杂的跨实体业务编排。

### 2. Application 层

承接：

- Task execution orchestration
- Story / Task context composition
- session plan building
- address space derivation use-case
- MCP injection planning

这里应该成为“系统用例”的主要落点。

### 3. Domain 层

继续保持：

- 纯实体 / 值对象
- 领域约束与验证
- repository trait
- 不感知 HTTP / WS / 前端返回结构

### 4. Infrastructure / Runtime Adapter

承接：

- relay transport
- executor connector
- repository 实现
- 外部服务访问

不在 domain 或 API 层里混入底层细节。

## Migration Suggestion

### Phase 1: 先识别边界并停止继续加重 API

- 新增或修改功能时，优先避免再把新业务直接塞进 route handler
- 对已有热点文件先做职责标注和拆分准备

### Phase 2: 抽 application use-case

优先迁移：

- task execution orchestration
- session plan building
- task/story context composition

让 API 只调用 application service。

### Phase 3: 引入 response assembler / DTO

对于容易直接暴露 domain 的接口，引入：

- response DTO
- assembler / mapper

让 transport 输出与领域模型脱钩。

### Phase 4: 继续收口 runtime composition

把 address space / MCP / runtime tool injection 的“用例拼装”放到 application 层，API 只保留调用入口。

## Non-Goals

- 一次性重写所有 crate 依赖关系
- 为了“理论分层漂亮”而拆出大量空壳模块
- 在没有明确收益的地方强行抽象
- 现在就重做全部 API response 模型

## Suggested Follow-up Tasks

1. `extract-task-execution-application-services`
   - 先把 task execution 相关业务编排从 API crate 抽离

2. `move-session-plan-and-context-composition-to-application`
   - 收口 session plan / context composer 的归属

3. `introduce-api-response-assemblers`
   - 为高频接口建立 DTO / assembler 边界

4. `thin-route-handlers-and-error-mapping`
   - 让 routes 只保留 transport 入口职责

## Related Files

- `.trellis/spec/backend/directory-structure.md`
- `.trellis/spec/backend/repository-pattern.md`
- `crates/agentdash-api/src/routes`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `crates/agentdash-api/src/task_agent_context.rs`
- `crates/agentdash-api/src/session_plan.rs`
- `crates/agentdash-application/src`
- `crates/agentdash-domain/src`
