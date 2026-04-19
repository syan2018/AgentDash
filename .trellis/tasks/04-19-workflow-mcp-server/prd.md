# Workflow MCP Server — 工作流管理工具集

## 背景

当前 Workflow/Lifecycle 的创建依赖手写 JSON 模板（如 `trellis_dev_task.json`），涉及嵌套的 injection/constraints/completion/hook_rules 结构，对 Agent 和人类用户都过于繁琐。需要一个 Project 级 MCP 工具集，让 Agent 能直接通过工具调用完成工作流的创建和编辑。

## 设计决策记录

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 操作范围 | 完整 Bundle 级 | 一站式创建可用工作流 |
| 抽象层级 | 直接操作对象（MCP 工具） | 对齐 Story/Task MCP Server 模式 |
| 工具粒度 | upsert（合并 CRUD） | 减少工具数量，Agent 更易理解 |
| 模板工具 | 不提供 | 内置模板后续要删，不建立依赖 |
| 验证策略 | 保存时验证 | 即时反馈，Agent 可修正 |
| Hook 脚本 | 完整 Rhai 支持 | 沙箱验证确保安全 |
| Lifecycle-Workflow 耦合 | 严格引用 | 必须先创建 workflow 再引用 |
| 绑定范围 | Project-bound | assign 是默认行为，不需单独工具 |

## 工具集设计

### MCP Server: `WorkflowMcpServer`

绑定到 Project，类似 `StoryMcpServer` 模式。

### 工具列表

#### 1. `list_workflows`
列出当前项目下所有 workflow 和 lifecycle 定义。

**返回**: workflow 列表（key, name, binding_kind, status）+ lifecycle 列表（key, name, steps 概要）

#### 2. `get_workflow`
获取单个 WorkflowDefinition 详情。

**参数**: `workflow_key: String`
**返回**: 完整的 WorkflowDefinition（含 contract 详情）

#### 3. `get_lifecycle`
获取单个 LifecycleDefinition 详情。

**参数**: `lifecycle_key: String`
**返回**: 完整的 LifecycleDefinition（含 steps, edges, entry_step）

#### 4. `upsert_workflow`
创建或更新 WorkflowDefinition。

**参数**:
- `key: String` — 唯一标识
- `name: String`
- `description: String`
- `binding_kind: String` — "project" | "story" | "task"
- `contract: WorkflowContractInput` — 包含:
  - `injection` — instructions, context_bindings
  - `constraints` — 行为约束规则列表
  - `completion` — 完成条件定义
  - `hook_rules` — Rhai 脚本规则（完整支持）
  - `recommended_ports` — 建议的输入/输出端口

**行为**: 
- key 存在则更新，不存在则创建
- 保存时执行完整验证（contract 结构、Rhai 脚本沙箱校验）
- 失败返回详细错误信息

#### 5. `upsert_lifecycle`
创建或更新 LifecycleDefinition，自动绑定到当前 Project。

**参数**:
- `key: String`
- `name: String`  
- `description: String`
- `binding_kind: String`
- `entry_step_key: String`
- `steps: Vec<StepInput>` — 每个 step 含 key, description, workflow_key（可选）, node_type, input_ports, output_ports
- `edges: Vec<EdgeInput>` — 每条边含 from_node, from_port, to_node, to_port

**行为**:
- 严格引用检查：step.workflow_key 不存在时拒绝
- DAG 拓扑验证 + port 契约检查
- 自动创建 WorkflowAssignment 绑定到当前 Project
- 失败返回详细错误信息

### Server Instructions

嵌入在 `with_instructions()` 中，内容包含：
1. Workflow/Lifecycle 领域模型概述
2. 每个工具的使用场景和参数说明
3. 推荐的创建流程（先 workflow → 后 lifecycle → 自动 assign）
4. Hook rules 编写指南（preset vs 自定义 Rhai）
5. 常见错误及修正方式

### 访问控制（当前阶段）

暂时直接加入可用工具集，所有 Agent 均可使用。后续通过 Task #04-19-dynamic-agent-capability-provisioning 实现 capability flag 控制。

## 实现路径

1. `crates/agentdash-mcp/src/servers/workflow.rs` — 新增 MCP Server
2. `crates/agentdash-mcp/src/servers/mod.rs` — 注册
3. `crates/agentdash-application/src/workflow/catalog.rs` — 可能需要扩展校验方法
4. `crates/agentdash-application/src/vfs/tools/provider.rs` — 注册到 tool provider 管线

## 关联 Task

- `04-19-session-tool-capability-pipeline` — 收口 session 工具能力管线
- `04-19-dynamic-agent-capability-provisioning` — 动态 Agent 能力管线
