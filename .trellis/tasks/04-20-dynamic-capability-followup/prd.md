# 动态能力赋予链路补齐与验收收尾

## Goal

承接 `04-19-dynamic-agent-capability-provisioning` 的剩余缺口，
将 lifecycle 动态能力赋予从“后端主链已具备”补齐到“配置可达、运行时可生效、验收可闭合”的状态。

本任务的目标不是重做整套能力模型，而是围绕以下四个现实缺口完成收尾：

- `agent_node` 新 session 的能力注入链路尚未完全闭合
- step capability 的前端配置/展示路径尚未打通
- capability key 仍缺少定义期校验
- 现有测试更偏 pure function，缺少真实集成链路兜底

## Scope Decision

### In Scope

- step / phase 切换时的能力更新语义
- `agent_node` 与 `phase_node` 的动态能力生效路径
- step capability 的前后端配置链路
- capability key 的定义期校验
- 能力更新相关的测试与验收闭环

### Out of Scope

- `SessionStart` 的能力说明注入

原因：
Agent 自身已经具备工具治理策略，初始化时不再由我们额外管理能力说明。
本任务只处理**能力更新事件**及其对应的运行时同步、配置入口与验收口径。

## Current Assessment

### 已完成主链

上一任务已经完成以下后端主体能力：

- `CapabilityDirective(Add/Remove)` 与 `compute_effective_capabilities()`
- `LifecycleStepDefinition.capabilities` 字段
- `phase_node` 切换时的 `CapabilityDelta`
- PiAgent MCP replace-set 热更新
- 结构化 delta Markdown 通知

因此，本任务不再重复实现这些基础设施，而是在其上补齐闭环。

### 剩余缺口

#### 1. `agent_node` 的真实生效链路不完整

当前 `agent_node` 创建新 session 时，虽然会把 step effective capabilities 传入 resolver，
但真实 orchestrator 路径中仍存在以下风险：

- 自定义 `mcp:*` 所需的 agent MCP server 列表未必完整传入
- `story_id` / `task_id` 等 owner scope 信息未必随新 session 正确带入
- 结果可能表现为“capability key 已声明，但 MCP tool 未真正进入 session”

这意味着当前链路更像“部分生效”，而不是“验收可闭合”。

#### 2. step capability 的前端路径缺失

目前前端至少存在以下断点：

- `LifecycleStepDefinition` 前端类型未完整承载 `capabilities`
- workflow/lifecycle 服务层映射未完整读写该字段
- DAG 编辑器与 side panel 缺少 step capability 的编辑入口
- step summary / 运行视图缺少必要展示

结果是后端即便支持 step capability，用户仍无法通过正常产品路径完成配置与验证。

#### 3. capability key 缺少定义期校验

当前行为更接近运行时容错：

- 非法 key 可能进入 lifecycle 定义
- resolver 在运行时静默忽略或告警
- 错误暴露时机偏晚

本任务需要把这类问题前移到定义写入阶段。

#### 4. 测试覆盖偏向静态链条

现有测试已经证明 pure function 链条基本正确，但还不足以兜住以下真实问题：

- orchestrator 到 resolver 的真实输入缺失
- `agent_node` 的 owner scope 漏传
- 前端 capability 字段遗漏
- 配置提交后未真正影响运行时

因此需要新增更贴近真实数据流与集成路径的测试。

## Requirements

### 1. `agent_node` 能力注入链路补齐

- [ ] 梳理 `agent_node` 创建 session 时的 resolver 输入来源
- [ ] 补齐 `agent_mcp_servers` 的真实传递链路
- [ ] 补齐 `story_id` / `task_id` / owner scope 等能力注入依赖信息
- [ ] 确保 step 声明的 `mcp:*`、`story_management`、`task_management` 等能力在真实路径上可生效
- [ ] 明确并固定“继承父 session 信息”和“仅使用 step effective capability 集合”之间的边界

### 2. step capability 前端配置路径补齐

- [ ] 为前端 `LifecycleStepDefinition` 类型补齐 `capabilities`
- [ ] 补齐 workflow/lifecycle 服务层的字段映射与序列化
- [ ] 在 DAG 编辑器 / side panel 中提供 step capability 的编辑入口
- [ ] 至少支持 `Add` / `Remove` directive 的最小可用编辑路径
- [ ] 在适当视图中展示 step capability，避免“已配置但不可见”

### 3. capability key 定义期校验补齐

- [ ] 在 lifecycle 定义校验阶段校验 step capability key
- [ ] 平台 key 必须属于 well-known 集合
- [ ] 自定义 key 必须满足 `mcp:<server_name>` 的基本格式
- [ ] 非法 key 必须在定义写入前失败，而不是运行时静默跳过

### 4. 更新事件治理口径对齐

- [ ] 在文档和实现中明确：`SessionStart` 不在本任务范围内
- [ ] 验收口径只围绕 step / phase 切换时的能力更新事件
- [ ] 保证 delta 通知、MCP replace-set、runtime capability tracking 三者语义一致

### 5. 测试与验收闭环

- [ ] 新增覆盖真实 orchestrator 路径的测试，而不只验证 pure function 链条
- [ ] 覆盖 `agent_node` + custom `mcp:*` 场景
- [ ] 覆盖 owner scope 相关平台能力场景
- [ ] 覆盖前端 capability 配置路径至少一条基本流转场景
- [ ] 覆盖非法 capability key 场景
- [ ] 覆盖能力更新事件对应的最终验收行为

## Priority

建议按以下顺序推进：

1. `agent_node` 后端生效链路补齐
2. 前端类型与 DTO 映射补齐
3. 前端编辑入口补齐
4. 定义期校验补齐
5. 集成测试与验收闭环

原因：
后端真实生效链路是最大风险点；在此之前补 UI 只能解决“能填”，不能解决“会生效”。

## Acceptance Criteria

- [ ] `agent_node` step 声明 `mcp:*` capability 时，新 session 能拿到对应 MCP server，而不只是 capability key
- [ ] `agent_node` step 声明依赖 owner scope 的平台能力时，真实 session 能拿到对应平台 MCP 能力
- [ ] step capability 至少存在一条完整的前端配置路径，不再只能依赖手工 payload 或后端直写
- [ ] 非法 capability key 在 lifecycle 定义阶段被拒绝
- [ ] `SessionStart` 已明确排除出本任务范围，代码、文档和测试均只围绕能力更新事件
- [ ] 测试能够覆盖真实集成链路，不再只依赖手工构造 resolver 输入
- [ ] 原 `04-19-dynamic-agent-capability-provisioning` 可作为阶段性完成任务归档，不再承担未闭合验收项

## Technical Notes

- 优先保持当前已落地的 replace-set / delta 结构不回退
- `agent_node` 注入链路优先复用已有 SessionPlanBuilder / project agent preset MCP 解析能力，而不是重新发明一套能力组装逻辑
- 前端 capability 支持优先走现有 workflow/lifecycle 编辑器与 DTO，不额外引入新的配置入口
- 不再要求补齐 `SessionStart` hook 注入；重点是 update path 的一致性与可验证性
