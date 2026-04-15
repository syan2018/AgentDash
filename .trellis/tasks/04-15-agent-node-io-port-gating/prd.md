# Agent Node I/O Port 体系 + Artifact 门禁 + 上下文构建器

## Goal

为 lifecycle DAG 中的 agent node 引入显式的 I/O port 体系，将节点产出从"事后报告"模式升级为"声明式交付"模式。每个 agent node 通过 output port 声明必须交付的产出（VFS 写入），通过 input port 声明依赖的前驱产出（VFS 读取 + 上下文注入），并由门禁机制保证交付质量。最终实现 lifecycle 节点间的结构化信息流动。

## Background

当前 lifecycle 系统的 artifact 体系基于 `report_workflow_artifact` 工具，agent 通过主动提交 artifact 来记录产出。但这套机制存在几个根本问题：

1. **产出是 "记录"而非 "交付"** — agent 提交的 artifact 没有结构化的接收方，后继 node 不会自动获得前驱产出
2. **产出位置不确定** — artifact 只是 append 到 run 的 record_artifacts 列表，不与 node 或 port 关联
3. **缺乏门禁** — 没有机制验证 agent 是否真的完成了要求的产出，advance_lifecycle_node 不检查交付物
4. **上下文断裂** — 后继 node 的 agent 需要手动通过 VFS 查找前驱产出，没有自动化的上下文构建

## Architecture Decisions

以下决策已在 brainstorm 阶段与用户确认：

### AD1: Port 定义在 WorkflowContract 上

I/O port 是 workflow 合约的一部分，而非 lifecycle 拓扑层的概念。

- `WorkflowContract` 新增 `output_ports: Vec<OutputPortDefinition>` 和 `input_ports: Vec<InputPortDefinition>`
- Output port 声明该 workflow 必须交付的产出及其门禁策略
- Input port 声明该 workflow 需要的外部输入及其上下文构建方式
- Port 定义与 lifecycle 节点解耦，同一个 workflow 可复用于不同 lifecycle 中的不同节点

### AD2: Port-Level Edge 连线

DAG edge 从 node 级别升级为 port 级别，明确指定哪个 output port 连接到哪个 input port。

- `LifecycleDefinition` 新增 `edges: Vec<LifecycleEdge>`
- `LifecycleEdge = { from_node, from_port, to_node, to_port }`
- 现有 `LifecycleStepDefinition.depends_on` 从 edges 自动推导（向后兼容计算属性）
- 验证：edge 引用的 port key 必须在对应 node 的 workflow contract 中存在

### AD3: 产出通过 VFS 写入，非专用工具

移除 `report_workflow_artifact` 工具。Agent 通过现有 write_file 工具直接写入 `lifecycle://` 命名空间。

- Output port 的本质是一个 VFS locator 声明
- VFS 命名空间：`lifecycle://outputs/{node_key}/{port_key}`（扁平结构）
- lifecycle mount 的 address space provider 需支持对 output path 的写入能力
- 需正确处理可写路径（仅当前 node 的 output port）与只读路径（其他 node 的 output、系统生成内容）的权限分离

### AD4: 门禁 Per Node，3 次碰壁上限

- Agent 调用 `advance_lifecycle_node` 时触发门禁检查
- 检查所有 output port 的交付状态，任一未满足即 gate collision
- 每个 node 最多 3 次 gate collision，超过后 node 标记为 Failed
- Agent 也可通过工具主动标记 node 为 Failed
- `LifecycleStepState` 新增 `gate_collision_count: u32`

### AD5: 门禁检查深度 — 标准化三档接口

- **Existence**（首期实现）：检查 VFS locator 是否存在非空内容
- **Schema**（预留接口）：检查内容是否符合预定义的 content_type / required_fields
- **LLM Judge**（预留接口）：配置 evaluation prompt，用 LLM 判断产出质量
- 三档通过 hook 策略注入（类似现有 WorkflowHookRuleSpec 的 preset 机制）

### AD6: Input Port 上下文构建 — 混合模板策略

- 预设策略：`full`（默认）| `summary` | `metadata_only`
- 允许覆盖为自定义 prompt 模板（Handlebars 风格，变量绑定 artifact 内容）
- 上下文构建器将 input port 内容、现有 context_bindings、instructions 合并注入
- Input port 内容注入顺序：lifecycle 位置上下文 → input port artifacts → context_bindings → instructions

### AD7: 旧 Artifact 体系移除

- 直接移除 `report_workflow_artifact` 工具及 `WorkflowRecordArtifact` 体系
- lifecycle:// 命名空间需同时支持写入内容和系统自动生成内容
- 基于 inline_fs 机制调整，正确处理可写/只读子路径

## Requirements

### R1: 数据模型 — Port 定义

在 `WorkflowContract` 上新增：

```rust
pub struct OutputPortDefinition {
    pub key: String,
    pub description: String,
    /// 门禁策略类型
    pub gate_strategy: GateStrategy,  // Existence | Schema | LlmJudge
    /// 策略参数（schema 的 required_fields、LLM 的 evaluation_prompt 等）
    pub gate_params: Option<serde_json::Value>,
}

pub struct InputPortDefinition {
    pub key: String,
    pub description: String,
    /// 上下文构建策略
    pub context_strategy: ContextStrategy,  // Full | Summary | MetadataOnly | Custom
    /// 自定义 prompt 模板（仅 Custom 策略使用）
    pub context_template: Option<String>,
}
```

### R2: 数据模型 — Port-Level Edge

在 `LifecycleDefinition` 上新增：

```rust
pub struct LifecycleEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}
```

- `depends_on` 变为从 edges 推导的计算属性
- 验证逻辑：edge 引用的 node key 和 port key 必须在 lifecycle 的 step 定义和对应 workflow contract 中存在

### R3: Lifecycle VFS 写入能力

- 修改 lifecycle mount 的 address space provider，支持对 `lifecycle://outputs/{current_node_key}/*` 路径的写入
- Agent 通过标准 write_file 工具写入，provider 将内容持久化
- 非当前 node 的 output 路径为只读
- 系统自动生成的内容路径（如执行日志、元数据）为只读

### R4: 门禁机制

- `advance_lifecycle_node` 工具在推进前检查当前 node 所有 output port 的交付状态
- 首期：检查每个 output port 对应的 `lifecycle://outputs/{node_key}/{port_key}` 是否有非空内容
- Gate collision 计数存储在 `LifecycleStepState.gate_collision_count`
- 3 次碰壁 → 自动标记 node 为 Failed
- Agent 可通过 `advance_lifecycle_node` 的 `mark_failed` 参数主动标记失败
- 门禁检查结果通过 tool response 返回给 agent，说明哪些 port 未满足

### R5: Agent Node 上下文构建器

- 在 agent node session 创建时，自动构建输入上下文：
  1. Lifecycle 位置上下文（当前 node 在 DAG 中的位置、已完成的前驱等）
  2. Input port artifacts（从前驱 node 的 output port VFS 路径读取，按 context_strategy 处理）
  3. Output port 要求（将需要交付的 output port 定义注入为指令，让 agent 知道需要产出什么）
  4. 现有 context_bindings / instructions
- 上下文构建结果注入到 session 的 bootstrap prompt 或 address space

### R6: 移除旧 Artifact 体系

- 移除 `report_workflow_artifact` / `artifact_report` 工具
- 移除或重构 `WorkflowRecordArtifact` 类型（评估是否有其他依赖方）
- 调整前端对 artifact 的展示，改为从 VFS locator 读取

## Acceptance Criteria

- WorkflowContract 支持声明 output_ports 和 input_ports
- LifecycleDefinition 支持 port-level edges，depends_on 从 edges 推导
- Agent 可通过 write_file 写入 `lifecycle://outputs/{node_key}/{port_key}` 路径
- 非当前 node 的 output 路径为只读，写入被拒绝
- advance_lifecycle_node 在推进前检查 output port 交付状态
- Gate collision 计数正确递增，3 次碰壁自动标记 Failed
- Agent node session 创建时自动注入 input port 上下文
- Input port 的 full/summary/metadata_only 预设策略工作正常
- report_workflow_artifact 工具已移除
- 前端 lifecycle 视图正确展示 port 状态和门禁信息
- 数据库 schema 迁移已配套
- GateStrategy 和 ContextStrategy 的 Schema/LlmJudge 等高级策略接口已预留

## Implementation Phases

### Phase 1: 数据模型 + Edge 重构

- 新增 port 和 edge 的 domain value objects
- 重构 LifecycleDefinition 验证逻辑，支持 edges + port 引用校验
- 数据库迁移

### Phase 2: VFS 写入能力 + 门禁

- Lifecycle mount provider 支持 output path 写入
- advance_lifecycle_node 集成门禁检查
- Gate collision 计数和 Failed 状态管理

### Phase 3: 上下文构建器

- Agent node session 创建时的上下文注入管线
- Input port → VFS 读取 → context strategy 处理 → prompt 注入
- Output port 要求注入

### Phase 4: 旧体系清理 + 前端适配

- 移除 report_workflow_artifact
- 前端 port 状态和门禁信息展示
- 编辑器集成（与后续 DAG Editor task 衔接）

## Technical Notes

- lifecycle:// 命名空间需同时支持写入（output port）和自动生成内容（元数据），需在 VFS provider 中明确区分可写/只读子路径
- Port-level edge 模型使得 lifecycle 定义变得更重，需要关注序列化/反序列化性能
- 门禁检查在 advance_lifecycle_node 工具中执行，而非 before_stop hook，因为它是"推进前验证"而非"阻止退出"
- 上下文构建器应与现有的 `resolve_active_workflow_projection_for_session` 和 `build_lifecycle_mount` 集成

## Related Tasks

- `04-13-lifecycle-dag-orchestration` — 前置任务，已完成 DAG 编排基础框架
- `04-15-dag-lifecycle-editor` — 后置任务，需要本 task 的 port/edge 模型来渲染编辑器
- `03-30-lifecycle-vfs-typed-access` — 相关，lifecycle VFS 访问能力增强

