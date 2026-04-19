# 动态 Agent 能力管线 — 基于 Lifecycle 的工具自动供给

## Goal

在 Phase 1 的 ToolCapability 声明模型基础上，实现 lifecycle step 级的动态工具供给。
核心原则：**能力说明通过 hook 管线注入（初始化/变化时），而非直接修改 tool desc 上下文**。

## Prerequisites

- `04-19-session-tool-capability-pipeline` (P1) 完成
- ToolCapability（开放 string key）+ CapabilityResolver 已就位
- 平台 well-known 能力 + `mcp:*` 用户自定义能力解析已就位
- MCP 注入已由 CapabilityResolver 统一治理
- SessionPlanBuilder 统一声明式 session 构造管线已就位

## Background

### 当前限制

Phase 1 完成后，session 的工具集在创建时一次性确定，全生命周期不变。
Lifecycle 场景需要在 step 切换时调整 agent 可用的工具能力：

- `agent_node`（新 session）：每个 step 对应独立 session，工具集在创建时确定，不需要 mid-session 变更
- `phase_node`（session 内切换）：同一 session 经历多个 phase，工具集需要动态调整

### 核心设计思路

1. Step 级声明 capabilities（CapabilityDirective: Add/Remove）
2. Hook 管线检测 step transition
3. 计算 CapabilityDelta
4. 通过 hook context injection 通知 agent 能力变更（结构化 Markdown）
5. Phase Node：扩展 PiAgentConnector 支持 MCP server 列表热更新

## Design Decisions

### Step 级 Capability 声明格式

采用 **CapabilityDirective 结构化枚举**（非裸字符串列表），支持 allowlist + denylist 语义：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDirective {
    Add(String),
    Remove(String),
}
```

**继承规则**：
- step 未声明 → 完全继承 workflow binding 级 capabilities
- step 声明了 directive → 在 workflow 基线上执行 Add/Remove 运算
- Add: 在基线上追加能力（如新增 `mcp:code_analyzer`）
- Remove: 从基线上移除能力（如暂时关闭 `file_system`）

### Session Init 触发方式

采用 `SessionStart` 的 sub-phase 机制：
- Hook provider 的 `load_session_snapshot` 已在 SessionStart 时被调用
- 在该阶段注入 step 的 effective capabilities 说明到 hook injection
- 不需要新的 trigger 类型

### Phase Node MCP 热插拔

扩展 `PiAgentConnector` 而非走 ACP 协议：
- PiAgent 是 in-process agent，可直接暴露 runtime MCP 列表更新接口
- 避免 ACP 消息协议的复杂度
- 由 HookSessionRuntime 检测 step transition 后调用更新接口

### Delta 通知格式

采用结构化 Markdown 注入 `capability` slot：

```markdown
## Capability Update — Step Transition: review → implement

### Added Capabilities
- **file_system**: Read, Write, Execute file operations
- **mcp:code_analyzer**: External code analysis tools

### Removed Capabilities
- **canvas**: Canvas rendering tools (no longer available)
```

## Requirements

### 2.1 Step 级 Capability 声明（域层）

- [x] `CapabilityDirective` 枚举：`Add(String)` / `Remove(String)`
- [x] `LifecycleStepDefinition` 新增 `capabilities: Vec<CapabilityDirective>`
- [x] `compute_effective_capabilities()` 计算函数：workflow baseline + step directives → effective set
- [x] 域层校验：平台 key 必须在 well-known 集合中；`mcp:*` key 格式合法即可
- [x] Workflow MCP tool `upsert_lifecycle_tool` 支持 step capabilities 输入
- [x] 数据库迁移（lifecycle_definition 已是 JSONB，字段扩展向前兼容）

### 2.4 Agent Node session 创建

- [ ] Orchestrator `create_agent_node_session` 读取 step effective capabilities
- [ ] 传入 `CapabilityResolverInput.workflow_capabilities` 字段
- [ ] Session 从创建时即具备正确工具集

### 2.5 Capability 变更追踪

- [ ] `HookSessionRuntime` 新增 `current_capabilities: BTreeSet<String>` 状态字段
- [ ] `CapabilityDelta { added: Vec<String>, removed: Vec<String> }` 结构
- [ ] Step transition 时比较新旧 effective capabilities 产出 delta

### 2.2 Hook 管线 Capability 注入

- [ ] `SessionStart` 阶段：hook provider 注入 effective capabilities 说明（`capability` slot）
- [ ] `capability_changed` 事件：step 切换后注入 delta Markdown
- [ ] 格式遵循上述 Design Decisions 中的结构化 Markdown

### 2.3 Phase Node mid-session 处理

- [ ] HookSessionRuntime 检测 step transition event
- [ ] 计算 CapabilityDelta
- [ ] 通过 hook injection 注入变更通知
- [ ] 扩展 PiAgentConnector 暴露 MCP 列表更新接口
- [ ] Phase transition 时通过该接口热更新 MCP server 列表

## Acceptance Criteria

- [ ] `agent_node` step 创建的 session 工具集包含 step 声明的 capabilities
- [ ] `phase_node` step 切换后，agent 收到 capability 变更通知（结构化 Markdown）
- [ ] capability 变更通知包含可操作的工具说明（agent 可据此调用新工具）
- [ ] MCP server 列表在 phase_node 切换时正确更新（PiAgent 热插拔）
- [ ] step 未声明 capabilities 时，正确继承 workflow 级
- [ ] CapabilityDirective.Remove 能从基线中移除能力
- [ ] 集成测试覆盖 agent_node + phase_node 两种场景

## Technical Notes

### 关键架构决策

| 决策 | 选择 | 原因 |
|------|------|------|
| Step capability 声明 | CapabilityDirective 枚举 (Add/Remove) | allowlist+denylist 灵活性，step 可加可减 |
| 能力通知方式 | hook 管线注入（capability slot） | 比直接 tool mutation 更灵活 |
| Session Init | SessionStart sub-phase | 复用现有 load_session_snapshot，零新 trigger |
| Phase Node MCP 更新 | 扩展 PiAgentConnector | in-process agent 直接更新，避免 ACP 复杂度 |
| Delta 通知格式 | 结构化 Markdown | agent 可读、可操作，包含能力描述 |
| step 无声明继承 | 继承 workflow 级 | 减少 lifecycle 定义的 boilerplate |
| agent_node vs phase_node | 不同处理路径 | agent_node 天然静态，phase_node 需动态 |

### 实施优先级

2.1（域层） → 2.4（Agent Node） → 2.5（Delta 追踪） → 2.2（Hook 注入） → 2.3（Phase Node 热插拔）

### 复杂度评估

- **预估工作量**: 3-5 sessions
- **主要风险**: PiAgentConnector MCP 热插拔的安全性
- **测试策略**: 域层单测 + Hook 集成测试 + lifecycle step transition 测试
