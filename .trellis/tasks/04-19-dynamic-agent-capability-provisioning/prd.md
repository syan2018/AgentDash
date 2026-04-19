# 动态 Agent 能力管线 — 基于 Lifecycle 的工具自动供给

## Goal

在 Phase 1 的 ToolCapability 声明模型基础上，实现 lifecycle step 级的动态工具供给。
核心原则：**能力说明通过 hook 管线注入（初始化/变化时），而非直接修改 tool desc 上下文**。

## Prerequisites

- `04-19-session-tool-capability-pipeline` (P1) 完成
- ToolCapability（开放 string key）+ CapabilityResolver 已就位
- 平台 well-known 能力 + `mcp:*` 用户自定义能力解析已就位
- MCP 注入已由 CapabilityResolver 统一治理

## Background

### 当前限制

Phase 1 完成后，session 的工具集在创建时一次性确定，全生命周期不变。
Lifecycle 场景需要在 step 切换时调整 agent 可用的工具能力：

- `agent_node`（新 session）：每个 step 对应独立 session，工具集在创建时确定，不需要 mid-session 变更
- `phase_node`（session 内切换）：同一 session 经历多个 phase，工具集需要动态调整

### 核心设计思路

不直接修改 PiAgentConnector 的工具重建逻辑。而是：

1. Step 级声明 capabilities
2. Hook 管线检测 step transition
3. 计算 CapabilityDelta
4. 通过 hook context injection 通知 agent 能力变更
5. MCP server 列表通过 ACP session update 动态更新

## Requirements

### 2.1 Step 级 Capability 声明

- [ ] `LifecycleStepDefinition` 新增 `capabilities: Option<Vec<String>>`（开放 string key）
- [ ] 继承规则：step 无声明 → 继承绑定 workflow 的 capabilities；step 有声明 → union 合并
- [ ] 支持平台 key（如 `workflow_management`）和用户 MCP key（如 `mcp:code_analyzer`）
- [ ] `WorkflowMcpServer.upsert_lifecycle_tool` 支持 step capabilities 输入
- [ ] 域层校验：平台 key 必须在 well-known 集合中；`mcp:*` key 格式合法即可（config 在运行时解析）

### 2.2 Hook 管线 Capability 注入

- [ ] `session_init` hook：首次进入 step 时，注入 effective capabilities 说明到 hook context
- [ ] `capability_changed` hook：step 切换导致 capabilities 变更时，注入 delta 通知
- [ ] Hook 注入格式设计：结构化描述新增/移除的能力及对应可用工具

### 2.3 Phase Node mid-session 处理

- [ ] Hook runtime 检测 `step_transition` event
- [ ] 计算 `CapabilityDelta`（新增 capabilities、移除 capabilities）
- [ ] 通过 hook context injection 通知 agent
- [ ] MCP server 变更：通过 ACP 协议 session update 通知列表变更

### 2.4 Agent Node session 创建

- [ ] Orchestrator 创建新 session 时，传入 step 的 effective capabilities
- [ ] CapabilityResolver 接受 step capabilities 参数
- [ ] Session 从创建时即具备正确工具集

### 2.5 Capability 变更追踪

- [ ] `HookSessionRuntime` 维护 `current_capabilities: BTreeSet<ToolCapability>`
- [ ] Step transition 时产出 `CapabilityDelta { added, removed }`
- [ ] `CapabilityDelta` 触发 hook context injection

## Acceptance Criteria

- [ ] `agent_node` step 创建的 session 工具集包含 step 声明的 capabilities
- [ ] `phase_node` step 切换后，agent 收到 capability 变更通知
- [ ] capability 变更通知包含可操作的工具说明（agent 可据此调用新工具）
- [ ] MCP server 列表在 phase_node 切换时正确更新
- [ ] step 未声明 capabilities 时，正确继承 workflow 级
- [ ] 集成测试覆盖 agent_node + phase_node 两种场景

## Technical Notes

### 关键架构决策

| 决策 | 选择 | 原因 |
|------|------|------|
| 能力通知方式 | hook 管线注入 | 比直接 tool mutation 更灵活，不需改 PiAgentConnector |
| step 无声明继承 | 继承 workflow 级 | 减少 lifecycle 定义的 boilerplate |
| step 有声明合并 | union | step 是 workflow 的特化，应该只加不减 |
| agent_node vs phase_node | 不同处理路径 | agent_node 天然静态，phase_node 需动态 |

### 复杂度评估

- **预估工作量**: 3-5 sessions
- **主要风险**: hook 管线的 capability injection 设计复杂度
- **测试策略**: Hook 集成测试 + lifecycle step transition 测试

### 待 Phase 1 完成后细化的内容

- CapabilityDelta 的具体结构
- Hook context injection 的 prompt 格式
- ACP session update 的 MCP 列表变更协议
- phase_node 内 MCP server 热插拔的技术可行性验证
