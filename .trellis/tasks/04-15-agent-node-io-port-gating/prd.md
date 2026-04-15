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

### AD2: Port-Level Edge 连线 + depends_on 直接移除

DAG edge 从 node 级别升级为 port 级别，明确指定哪个 output port 连接到哪个 input port。

- `LifecycleDefinition` 新增 `edges: Vec<LifecycleEdge>`
- `LifecycleEdge = { from_node, from_port, to_node, to_port }`
- **直接移除** `LifecycleStepDefinition.depends_on`，不保留兼容字段（项目未上线，无旧数据兼容需求）
- node 级别依赖关系在运行时从 edges 计算：`fn node_deps_from_edges(edges) -> HashMap<to_node, Set<from_node>>`
- 线性场景也必须声明 edge，无 edge 退化为无依赖（所有 node 都 Ready）
- 验证：edge 引用的 node key 和 port key 必须在 lifecycle 的 step 定义和对应 workflow contract 中存在

### AD3: 产出通过 VFS 写入 — 扁平 artifacts 命名空间

移除 `report_workflow_artifact` 工具。Agent 通过现有 write_file 工具直接写入 `lifecycle://` 命名空间。

- Output port 的本质是一个 VFS locator 声明
- VFS 路径扁平化：`lifecycle://artifacts/{port_key}`，**不含 node_key 层级**
- port_key 在整个 lifecycle 内全局唯一（由 edge 引用关系隐含保证）
- lifecycle mount 的 address space provider 需支持对 `artifacts/`* 子路径的写入能力
- 可写范围：当前 node 的 output port 对应的 `artifacts/{port_key}` 路径
- 只读范围：其他 node 的 output port 路径、系统自动生成内容

### AD4: 门禁 = Rhai Hook 脚本，3 次碰壁上限

门禁的本质是 Rhai Hook 脚本，复用现有 hook 基础设施，不做硬编码特殊分支。

- 门禁通过 before_stop / advance gate hook 触发
- Hook 内部通过 VFS read 验证 `lifecycle://artifacts/{port_key}` 是否有内容
- 不走 "直接查 port_outputs 字段" 的捷径——agent 能看到的视图 = 门禁验证的视图
- 如果 VFS read 路径存在循环依赖，说明设计有缺陷，需单独拉 task 修复，不做 workaround
- 每个 node 最多 3 次 gate collision，超过后标记 Failed
- Agent 也可主动标记 Failed
- `LifecycleStepState` 新增 `gate_collision_count: u32`

### AD5: 门禁检查深度 — Rhai Hook Preset 实现

GateStrategy enum 保留在 OutputPortDefinition 上作为声明性元数据，实际检查逻辑由对应的 Rhai preset 实现：

- **Existence**（首期实现）：Rhai preset 检查 VFS locator 是否存在非空内容
- **Schema**（预留接口）：Rhai preset 检查内容是否符合 content_type / required_fields
- **LLM Judge**（预留接口）：Rhai preset 调用 evaluation prompt 判断产出质量
- preset 注册方式与现有 `WorkflowHookRuleSpec` 的 preset 机制一致

### AD6: Input Port 上下文 → 统一到 prompt builder 的 locator 解析流程

Input port 上下文注入不新建独立 context builder 模块，统一到标准的 locator 占位 + 自动解析路径。

- Input port 定义在 session build 阶段自动生成 `WorkflowContextBinding` 条目
  - locator = `lifecycle://artifacts/{source_port_key}`（从 edge 的 from_port 推导）
  - reason = 来自 input port 的 description
  - context_strategy / context_template 控制注入方式
- 这些 binding 与 workflow contract 中已有的 context_bindings 合并后，走标准的 `resolve_context_bindings()` 流程
- 不需要新建 `context_builder.rs` 模块——复用现有 prompt builder 路径
- Output port 的交付要求通过 workflow contract 的 instructions 注入（告诉 agent 需要写入哪些 `lifecycle://artifacts/{port_key}`）
- 预设策略：`Full`（默认）| `Summary` | `MetadataOnly` | `Custom`
- `Custom` 策略允许覆盖为自定义 prompt 模板（Handlebars 风格，变量绑定 artifact 内容）

### AD7: 旧 Artifact 体系移除

- 直接移除 `report_workflow_artifact` 工具及 `WorkflowRecordArtifact` 体系
- lifecycle:// 命名空间需同时支持写入内容和系统自动生成内容
- 基于 inline_fs 机制调整，正确处理可写/只读子路径

### AD8: Port Output 持久化 — 复用 inline_fs overlay 模式

- `LifecycleMountProvider` 的写入走 `InlineContentOverlay` 式的 write-through cache
- 底层持久化到 `LifecycleRun.port_outputs: BTreeMap<String, String>`（port_key → content）
- `LifecycleRun` 已是 JSON 大字段序列化，新增 port_outputs 无额外 schema 迁移
- 如后续体积膨胀，再拆独立表
- `LifecycleMountProvider` 不需要注入 `SessionBindingRepository`（因路径中不含 node_key）

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

### R2: 数据模型 — Port-Level Edge + depends_on 移除

在 `LifecycleDefinition` 上新增：

```rust
pub struct LifecycleEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}
```

- **直接移除** `LifecycleStepDefinition.depends_on` 字段
- node 级别依赖关系通过运行时函数 `node_deps_from_edges(edges)` 计算
- 验证逻辑：edge 引用的 node key 和 port key 必须在 lifecycle 的 step 定义和对应 workflow contract 中存在

### R3: Lifecycle VFS 写入能力

- 修改 lifecycle mount 的 address space provider，支持对 `lifecycle://artifacts/{port_key}` 路径的写入
- Agent 通过标准 write_file 工具写入，provider 通过 overlay + persister 将内容持久化到 `LifecycleRun.port_outputs`
- 可写范围：当前 node 的 output port 对应的 `artifacts/{port_key}` 路径
- 其他 node 的 output port 路径为只读
- 系统自动生成的内容路径（如执行日志、元数据）为只读

### R4: 门禁机制 — Hook 实现

- 门禁通过 Rhai Hook 脚本触发，复用现有 hook 基础设施（before_stop / advance gate）
- Hook 内部通过 VFS read 检查 `lifecycle://artifacts/{port_key}` 是否有非空内容
- 首期实现 Existence 策略的 Rhai preset
- Gate collision 计数存储在 `LifecycleStepState.gate_collision_count`
- 3 次碰壁 → 自动标记 node 为 Failed
- Agent 可通过 `advance_lifecycle_node` 的 `mark_failed` 参数主动标记失败
- 门禁检查结果通过 hook 返回的 gate response 传回给 agent，说明哪些 port 未满足

### R5: Input Port 上下文注入 — 统一到 prompt builder

- 在 agent node session build 阶段，input port 自动生成 `WorkflowContextBinding` 条目
  - locator = `lifecycle://artifacts/{source_port_key}`（从 edge 的 from_port 推导）
  - reason = input port 的 description
  - context_strategy 控制注入方式（Full / Summary / MetadataOnly / Custom）
- 这些 binding 与 workflow contract 中已有的 context_bindings 合并，走标准 `resolve_context_bindings()` 流程
- Output port 的交付要求通过 workflow contract 的 instructions 自动追加（告诉 agent 需要写入哪些 `lifecycle://artifacts/{port_key}`）
- Lifecycle 位置上下文（当前 node 在 DAG 中的位置、已完成的前驱等）保持现有注入方式

### R6: 移除旧 Artifact 体系

- 移除 `report_workflow_artifact` / `artifact_report` 工具
- 移除或重构 `WorkflowRecordArtifact` 类型（评估是否有其他依赖方）
- 调整前端对 artifact 的展示，改为从 VFS locator 读取

## Acceptance Criteria

- WorkflowContract 支持声明 output_ports 和 input_ports
- LifecycleDefinition 支持 port-level edges，`depends_on` 字段已移除
- edges 是 DAG 拓扑唯一数据源，node 级别依赖通过运行时函数计算
- Agent 可通过 write_file 写入 `lifecycle://artifacts/{port_key}` 路径
- 非当前 node 的 output port 路径为只读，写入被拒绝
- 门禁检查通过 Rhai Hook 实现，首期为 Existence 策略 preset
- Gate collision 计数正确递增，3 次碰壁自动标记 Failed
- Agent node session 创建时 input port 自动生成 context_bindings，走标准 resolve 流程
- Input port 的 Full/Summary/MetadataOnly/Custom 策略工作正常
- report_workflow_artifact 工具已移除
- 前端 lifecycle 视图正确展示 port 状态和门禁信息
- port_outputs 持久化在 LifecycleRun JSON 中，无需独立表
- GateStrategy 和 ContextStrategy 的 Schema/LlmJudge 等高级策略接口已预留

## Implementation Phases

### Phase 1: 数据模型 + Edge 重构

- 新增 Port/Edge/Strategy 类型到 `value_objects.rs`
- 移除 `LifecycleStepDefinition.depends_on`
- `LifecycleDefinition` 新增 `edges: Vec<LifecycleEdge>`
- `LifecycleRun` 新增 `port_outputs: BTreeMap<String, String>`、步骤级 `gate_collision_count`
- 新增 `node_deps_from_edges()` 运行时函数替代 depends_on
- 重构所有引用 depends_on 的编译错误
- 重构 DAG 验证逻辑，支持 edge-based 拓扑校验 + port 引用校验
- 前端 types/workflow.ts 补齐 port/edge 类型
- 内置模板 trellis_dag_task.json 更新

### Phase 2: VFS 写入能力 + 门禁（Rhai Hook）

- `LifecycleMountProvider` 支持对 `artifacts/{port_key}` 写入，overlay + persister 持久化到 LifecycleRun
- 新增 Existence 策略的 Rhai gate preset 脚本
- Gate collision 计数和 Failed 状态管理
- before_stop / advance gate hook 注册

### Phase 3: Input Port 上下文注入

- orchestrator / session_runtime_inputs 在 session build 阶段基于 input port + edge 生成 context_bindings
- context_bindings 走标准 resolve_context_bindings() 流程
- Output port 交付要求自动追加到 instructions
- hook snapshot 中包含 port 定义和交付状态

### Phase 4: 旧体系清理 + 前端适配

- 移除 report_workflow_artifact 工具及相关类型
- 前端 port 状态和门禁信息展示
- 编辑器集成（与后续 DAG Editor task 衔接）

## Technical Notes

- lifecycle:// 命名空间需同时支持写入（output port）和自动生成内容（元数据），需在 VFS provider 中明确区分可写/只读子路径
- Port-level edge 模型使得 lifecycle 定义变得更重，需要关注序列化/反序列化性能
- 门禁检查通过 Rhai Hook 实现，与现有 before_stop hook 同一基础设施，不在 advance_lifecycle_node 中做硬编码检查
- Input port 上下文不走独立 builder，与现有的 `resolve_active_workflow_projection_for_session` 和 `build_lifecycle_mount` 以及 `resolve_context_bindings()` 集成
- port_key 在整个 lifecycle scope 内全局唯一——定义验证时需校验跨 node 的 port_key 不重复（或至少不与 edge 引用产生歧义）
- 如果 VFS read 路径在门禁 hook 中存在循环依赖，视为设计缺陷，需单独 task 修复

## Related Tasks

- `04-13-lifecycle-dag-orchestration` — 前置任务，已完成 DAG 编排基础框架
- `04-15-dag-lifecycle-editor` — 后置任务，需要本 task 的 port/edge 模型来渲染编辑器
- `03-30-lifecycle-vfs-typed-access` — 相关，lifecycle VFS 访问能力增强

