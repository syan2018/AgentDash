# Gap Analysis: 当前实现 → I/O Port 体系目标

> 本文档逐层盘点当前代码状态与 PRD 目标之间的真实 gap，作为开发执行的指导依据。
> 每个 Gap 标注影响范围（domain / application / infrastructure / api / frontend）和预估改动规模。

---

## 一、Domain Layer — 数据模型 Gap

### Gap D1: WorkflowContract 缺少 Port 定义

**当前状态**

```rust
// value_objects.rs
pub struct WorkflowContract {
    pub injection: WorkflowInjectionSpec,
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
    pub constraints: Vec<WorkflowConstraintSpec>,
    pub completion: WorkflowCompletionSpec,
}
```

没有 `output_ports` / `input_ports` 字段。

**目标**

```rust
pub struct WorkflowContract {
    pub injection: WorkflowInjectionSpec,
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
    pub constraints: Vec<WorkflowConstraintSpec>,
    pub completion: WorkflowCompletionSpec,
    pub output_ports: Vec<OutputPortDefinition>,  // NEW
    pub input_ports: Vec<InputPortDefinition>,    // NEW
}
```

**需新增的 Value Objects**：


| 类型                     | 字段                                                           | 说明        |
| ---------------------- | ------------------------------------------------------------ | --------- |
| `OutputPortDefinition` | `key`, `description`, `gate_strategy`, `gate_params`         | 声明必须交付的产出 |
| `InputPortDefinition`  | `key`, `description`, `context_strategy`, `context_template` | 声明依赖的外部输入 |
| `GateStrategy`         | enum: `Existence`, `Schema`, `LlmJudge`                      | 门禁检查深度    |
| `ContextStrategy`      | enum: `Full`, `Summary`, `MetadataOnly`, `Custom`            | 上下文构建策略   |


**影响**：domain 层新增 ~60 行 value objects + WorkflowContract struct 扩展 + validate_contract 扩展。

**向后兼容**：所有新字段 `#[serde(default)]`，现有 JSON 定义无需变更即可反序列化。

---

### Gap D2: LifecycleDefinition 缺少 Edge 模型

**当前状态**

```rust
// entity.rs
pub struct LifecycleDefinition {
    // ...
    pub entry_step_key: String,
    pub steps: Vec<LifecycleStepDefinition>,
    // 无 edges 字段
}

// value_objects.rs
pub struct LifecycleStepDefinition {
    pub key: String,
    pub description: String,
    pub workflow_key: Option<String>,
    pub node_type: LifecycleNodeType,
    pub depends_on: Vec<String>,  // node 级别依赖，非 port 级别
}
```

DAG 依赖通过 `depends_on` 建立（node 级），验证逻辑在 `validate_dag_topology()` 中。

**目标**

```rust
pub struct LifecycleDefinition {
    // ...
    pub steps: Vec<LifecycleStepDefinition>,
    pub edges: Vec<LifecycleEdge>,  // NEW: port 级别连线
}

pub struct LifecycleEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}
```

**决策修订**：`depends_on` **直接移除**（不保留兼容字段），edges 是唯一 DAG 拓扑数据源。

**影响**：

- `LifecycleStepDefinition` 移除 `depends_on` 字段
- `LifecycleDefinition` struct 新增 `edges: Vec<LifecycleEdge>` 字段
- `LifecycleEdge` value object 新增
- 新增运行时函数 `node_deps_from_edges(edges) -> HashMap<to_node, Set<from_node>>` 替代 depends_on
- `validate_lifecycle_definition()` 扩展：edge 引用的 node key 有效性检查
- `validate_dag_topology()` 改写为基于 edges 的拓扑校验（单套逻辑，无兼容分支）
- `LifecycleRun::new()` 中从 edges 推导初始 `step_states` 的 ready 集合
- 所有引用 `depends_on` 的编译错误需逐一修复
- **重要**：edge 的 port 引用校验需要 **跨实体** 查询（edge 引用的 port 定义在 workflow contract 中），这个校验应放在 application 层而非 domain 纯验证函数中

---

### Gap D3: LifecycleStepState 缺少 Gate Collision 计数

**当前状态**

```rust
pub struct LifecycleStepState {
    pub step_key: String,
    pub status: LifecycleStepExecutionStatus,
    pub session_id: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub context_snapshot: Option<Value>,
}
```

**目标**

```rust
pub struct LifecycleStepState {
    // ...existing...
    pub gate_collision_count: u32,  // NEW: 门禁碰壁次数
}
```

**影响**：1 个字段新增，`#[serde(default)]` 兼容。

---

### Gap D4: WorkflowRecordArtifact 体系将被替代

**当前状态**

`WorkflowRecordArtifact` 及 `WorkflowRecordArtifactType` 是一套独立的产出记录体系，内嵌在 `LifecycleRun.record_artifacts` 中。它被以下组件使用：


| 文件                                                                              | 依赖方式          |
| ------------------------------------------------------------------------------- | ------------- |
| `entity.rs` — `LifecycleRun.record_artifacts`                                   | 数据存储          |
| `value_objects.rs` — `WorkflowRecordArtifact`, `WorkflowRecordArtifactType`     | 类型定义          |
| `tools/artifact_report.rs` — `WorkflowArtifactReportTool`                       | Agent 工具入口    |
| `run.rs` — `append_step_artifacts`, `WorkflowRecordArtifactDraft`               | 写入逻辑          |
| `provider_lifecycle.rs` — artifacts 路径族                                         | VFS 读取        |
| `hooks/workflow_snapshot.rs` — artifact 计数快照                                    | Hook 元数据      |
| `hooks/completion.rs` — artifact 条件检查                                           | Completion 评估 |
| `routes/workflows.rs` — API endpoints                                           | HTTP 接口       |
| 前端: `task-workflow-panel.tsx`, `workflow.ts`, `workflowStore.ts`, `workflow.ts` | UI 展示         |


**目标**：用 VFS 写入（port-based locator）替代。`record_artifacts` 字段和相关类型最终移除。

**策略**：分阶段处理

1. Phase 1-3: 新建 port/VFS 写入通路，旧体系暂时保留
2. Phase 4: 移除旧体系，清理所有依赖方

**风险**：这是改动面最广的一块，涉及 ~15 个文件，需谨慎分步处理。

---

## 二、Application Layer — 业务逻辑 Gap

### Gap A1: Lifecycle VFS Provider 不支持写入

**当前状态**

```rust
// provider_lifecycle.rs
async fn write_text(...) -> Result<(), MountError> {
    Err(MountError::NotSupported("lifecycle_vfs 不支持写入".to_string()))
}
```

`LifecycleMountProvider` 完全只读。

**目标**：支持对 `artifacts/{port_key}` 路径的写入（扁平，不含 node_key）。

**决策修订**：

1. `write_text()` 解析路径，判断是否为 `artifacts/{port_key}` 格式
2. 当前 session 的可写 port_key 范围 = 当前 node 的 output port 定义中的 key（从 WorkflowContract 获取）
3. 持久化存储：写入 `LifecycleRun.port_outputs: BTreeMap<String, String>`，overlay + persister 模式
4. 因路径中不含 node_key，`LifecycleMountProvider` **不需要**注入 `SessionBindingRepository`
5. 只需知道 "当前 session 对应的 node 的 output port keys" → 在 mount context 中传入即可

**影响**：`provider_lifecycle.rs` 的 `write_text` / `list` / `read_text` 需大幅扩展以支持 `outputs/` 路径族。

**需回答的关键问题**：写入是否需要 `MountOperationContext` 中携带当前 node_key？当前 context 只有 `session_id`，需要通过 session_association 反查 node_key。

---

### Gap A2: advance_lifecycle_node 无门禁检查

**当前状态**

```rust
// advance_node.rs execute()
// 直接调用 service.complete_step(...)
// 没有任何 output port 交付检查
```

`advance_lifecycle_node` 工具只做 step 完成推进，不检查 output port 是否已写入。

**决策修订**：门禁检查**不在 advance_node.rs 中硬编码**，改为通过 Rhai Hook 实现。

**修订后方案**：

1. 新增 Rhai gate preset 脚本（类似现有 `stop_gate_lifecycle_advance.rhai`）
2. Hook 内通过 VFS read 检查 `lifecycle://artifacts/{port_key}` 是否有非空内容
3. Hook 返回 gate response，包含未满足 port 列表
4. `advance_node.rs` 负责 collision_count 递增和 Failed 状态管理（基于 hook 返回结果）
5. collision_count >= 3 → 自动标记 node 为 Failed

**影响**：`advance_node.rs` 增加 collision 计数和 Failed 管理逻辑（~~30 行）。门禁判定逻辑移入 Rhai preset（~~20 行）。

---

### Gap A3: Orchestrator 不注入 Input Port 上下文

**当前状态**

```rust
// orchestrator.rs — start_agent_node_prompt()
let kickoff_prompt = format!(
    "你正在执行 lifecycle `{lifecycle_key}` 的 node {node_title}。\n\
请先完成当前阶段工作，并在完成后调用 `advance_lifecycle_node` 工具提交总结与产物。"
);
```

Orchestrator 创建 agent node session 时只注入了一个简单的启动 prompt，没有：

1. Input port artifacts 的自动上下文注入
2. Output port 要求的注入（告诉 agent 需要产出什么）
3. Lifecycle 位置上下文的注入

**决策修订**：**不新建 `AgentNodeContextBuilder` 模块**，统一到现有 prompt builder 的 locator 解析流程。

**修订后方案**：

1. orchestrator 在 session build 时，基于 input port 定义 + edge 的 from_port 推导，自动生成 `WorkflowContextBinding` 条目
  - locator = `lifecycle://artifacts/{source_port_key}`
  - reason = input port description
  - context_strategy 控制注入方式
2. 这些 binding 与 workflow contract 中已有的 context_bindings 合并，走标准 `resolve_context_bindings()` 流程
3. Output port 交付要求通过 instructions 追加注入
4. Lifecycle 位置上下文保持现有注入方式

**影响**：`orchestrator.rs` 的 session build 方法增加 context_bindings 生成逻辑（~30 行），无需新模块。

---

### Gap A4: session_runtime_inputs 不感知 Port

**当前状态**

`session_runtime_inputs.rs` 中 `build_task_session_runtime_inputs()` 只挂载 lifecycle VFS mount，不解析 port 相关的上下文。

**决策修订**：Input port 上下文统一到 `WorkflowContextBinding` → `resolve_context_bindings()` 标准流程。

**修订后方案**：由 orchestrator 在 session build 时生成 context_bindings 条目，`session_runtime_inputs` 只需正常处理 bindings 列表中带 `lifecycle://artifacts/`* locator 的条目（标准 resolve 流程已覆盖）。`session_runtime_inputs` 本身不需要特殊的 port 感知逻辑。

**影响**：主要改动在 orchestrator 侧（Gap A3），`session_runtime_inputs` 无需额外修改或修改量极小。

---

### Gap A5: Hook Snapshot 不包含 Port 状态

**当前状态**

`workflow_snapshot.rs` 生成的 `SessionHookSnapshot.metadata.active_workflow` 包含 `run_id`, `step_key`, `lifecycle_key` 等，但没有 port 定义和交付状态。

**目标**：snapshot 中需包含当前 node 的 output port 定义和交付状态，供 hook evaluation 和 agent 工具使用。

**影响**：`workflow_snapshot.rs` + `hooks.rs`（SPI 层 snapshot 类型扩展）。

---

## 三、Infrastructure Layer — 持久化 Gap

### Gap I1: LifecycleDefinition 序列化不含 edges

**当前状态**

PostgreSQL workflow_repository 中 `lifecycle_definitions` 表将 LifecycleDefinition 以 JSON 存储。由于 `LifecycleDefinition` struct 目前无 `edges` 字段，存储中也不含。

**影响**：仅需确保新字段 `#[serde(default)]`，现有数据反序列化不会失败。**无需迁移**（新字段默认空数组）。

### Gap I2: LifecycleRun 需要 port_outputs 存储

**决策**：`LifecycleRun` 新增 `port_outputs: BTreeMap<String, String>`（port_key → content），内嵌在 JSON 大字段中。

- 无需独立表，无需数据库迁移（`#[serde(default)]` 兼容）
- key 直接用 port_key（全局唯一，扁平路径）
- 后续如体积膨胀再拆表

---

## 四、API Layer — 接口 Gap

### Gap API1: Workflow/Lifecycle CRUD API 不感知 Port

当前 `routes/workflows.rs` 中的 CRUD 操作直接序列化/反序列化 domain 类型，新增 port 和 edge 字段后 API 自动生效（JSON pass-through）。

**影响**：**无额外代码改动**，但需确保前端表单能正确编辑新字段。

### Gap API2: 缺少 Port Output 查询 API

目前没有 API 让前端查询某个 node 的 port output 交付状态。

**目标**：新增或扩展 lifecycle run API，返回 port output 状态（哪些 port 已写入，哪些未写入）。

---

## 五、Frontend — UI Gap

### Gap F1: LifecycleSessionView 不展示 Port 状态

`lifecycle-session-view.tsx` 中的 `LifecycleNodeCard` 只展示 step 状态和 session 消息流，没有 port 信息。

**目标**：在 node 卡片中展示 output port 交付状态和 gate collision 信息。

### Gap F2: TaskWorkflowPanel 使用旧 Artifact 体系

`task-workflow-panel.tsx` 中的 `CategorizedArtifacts` 和 `buildCompletionArtifacts` 基于 `WorkflowRecordArtifact`，后续需要替换为 port-based 展示。

### Gap F3: 前端类型定义缺少 Port/Edge 类型

`types/workflow.ts` 中没有 `OutputPortDefinition`、`InputPortDefinition`、`LifecycleEdge` 等类型。

### Gap F4: workflowStore 不管理 Port Output 状态

`workflowStore.ts` 中的 state 没有 port output 相关数据。

---

## 六、Builtin 模板 Gap

### Gap B1: trellis_dag_task.json 无 Port 定义

当前内置 DAG 模板的 workflow contract 中没有 output_ports / input_ports：

```json
{
  "contract": {
    "injection": { "instructions": [...] },
    "hook_rules": [...],
    "completion": { "default_artifact_type": "phase_note" }
    // 缺少 output_ports / input_ports
  }
}
```

**目标**：为 research workflow 添加 output port（如 `research_report`），为 implement workflow 添加 input port（消费 research 的 output）。lifecycle 中添加 edge 连线。

---

## 七、实施优先级与依赖关系

```
Phase 1: 数据模型（D1 + D2 + D3 + I1）
    ↓
Phase 2: VFS 写入 + 门禁（A1 + A2 + I2）
    ↓
Phase 3: 上下文构建器（A3 + A4 + A5）
    ↓
Phase 4: 旧体系清理 + 前端适配（D4 + API2 + F1~F4 + B1）
```

### Phase 1 详细清单


| 文件                                 | 改动                                                                                                                                                                                  |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `domain/workflow/value_objects.rs` | 新增 `OutputPortDefinition`, `InputPortDefinition`, `GateStrategy`, `ContextStrategy`, `LifecycleEdge`；扩展 `WorkflowContract`；扩展 `validate_contract` 和 `validate_lifecycle_definition` |
| `domain/workflow/entity.rs`        | `LifecycleDefinition` 新增 `edges` 字段；`LifecycleStepState` 新增 `gate_collision_count`；`LifecycleRun::new()` 从 edges 推导初始状态                                                             |
| `domain/workflow/mod.rs`           | 导出新类型                                                                                                                                                                               |
| `frontend/src/types/workflow.ts`   | 新增 port/edge 类型定义                                                                                                                                                                   |
| 内置模板 JSON                          | 更新 port 和 edge 定义                                                                                                                                                                   |


### Phase 2 详细清单（已按决策修订）


| 文件                                                | 改动                                                                                             |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `domain/workflow/entity.rs`                       | `LifecycleRun` 新增 `port_outputs: BTreeMap<String, String>`                                     |
| `application/address_space/provider_lifecycle.rs` | `write_text` 支持 `artifacts/{port_key}` 路径（扁平，无 node_key）；`read_text` 和 `list` 扩展 artifacts 路径族 |
| 新增 Rhai preset 脚本                                 | Existence 策略门禁检查，VFS read `lifecycle://artifacts/{port_key}`                                   |
| `application/hooks/presets.rs`                    | 注册新的 gate check preset                                                                         |
| `application/workflow/run.rs`                     | gate_collision_count 递增和 Failed 状态管理                                                           |
| 无数据库迁移                                            | port_outputs 内嵌在 LifecycleRun JSON 中                                                           |


### Phase 3 详细清单（已按决策修订）


| 文件                                           | 改动                                                                 |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `application/workflow/orchestrator.rs`       | session build 时基于 input port + edge 生成 `WorkflowContextBinding` 条目 |
| ~~新文件 `context_builder.rs`~~                 | **不新建**——复用现有 prompt builder 的 `resolve_context_bindings()` 流程     |
| `application/hooks/workflow_snapshot.rs`     | Snapshot 中包含 port 定义和交付状态                                          |
| `spi/src/hooks.rs`                           | Snapshot 类型扩展                                                      |
| `application/task/session_runtime_inputs.rs` | Input port 上下文通过 context_bindings 标准流程处理                           |


### Phase 4 详细清单


| 文件                                                 | 改动                                   |
| -------------------------------------------------- | ------------------------------------ |
| 移除 `application/workflow/tools/artifact_report.rs` | 整个文件                                 |
| `application/workflow/tools/mod.rs`                | 移除 artifact_report 导出                |
| `application/address_space/tools/provider.rs`      | 移除 WorkflowArtifactReportTool 注册     |
| `domain/workflow/value_objects.rs`                 | 评估移除 `WorkflowRecordArtifact`* 类型    |
| `domain/workflow/entity.rs`                        | 评估移除 `LifecycleRun.record_artifacts` |
| `application/hooks/completion.rs`                  | 移除 artifact 条件检查                     |
| `routes/workflows.rs`                              | 移除 artifact 相关 API                   |
| 前端多文件                                              | 替换 artifact 展示为 port output 展示       |


---

## 八、关键设计决策（已确认）

> 以下决策在 2025-04-15 brainstorm 阶段与用户确认。

### Q1: Port Output 持久化 → 复用 inline_fs 模式

**决策**：Lifecycle VFS 的 output 写入复用现有 inline_fs 的 overlay + persister 模式。

- `LifecycleMountProvider` 的写入走 `InlineContentOverlay` 式的 write-through cache
- 底层持久化写入 `LifecycleRun` 的 `port_outputs: BTreeMap<String, String>`（port_key → content）
- `LifecycleRun` 已是 JSON 大字段序列化，新增 port_outputs 无额外 schema 迁移
- 如后续体积膨胀，再拆独立表

### Q2: VFS 路径扁平化 → 不含 node_key 层级

**决策**：Output artifact 的 VFS 路径为 `lifecycle://artifacts/{port_key}`，不含 node_key 维度。

- 信息更流通，后继 node 直接按 port_key 引用，无需知道前驱 node key
- port_key 在整个 lifecycle 内必须全局唯一（由 edge 引用关系隐含保证）
- 消除了 "VFS 写入时需要反查 current_node_key" 的问题
- `LifecycleMountProvider` 不再需要注入 `SessionBindingRepository`

### Q3: depends_on 直接移除 → edges 唯一数据源

**决策**：`LifecycleStepDefinition.depends_on` 直接移除，不保留兼容字段。

- 项目完全未上线，不需要兼容旧数据
- edges 是唯一的 DAG 拓扑数据源
- node 级别依赖关系在运行时从 edges 计算（`fn node_deps_from_edges(edges) -> HashMap<to_node, Set<from_node>>`）
- 验证逻辑只需一套基于 edges 的分支
- 无 edges 时（空 Vec）退化为无依赖（所有 node 都 Ready），等价于旧的线性顺序推进被废弃——线性场景也必须声明 edge

### Q4: 门禁检查 → 必须走 VFS + Hook（Rhai 脚本）

**决策**：门禁本质是 Rhai Hook 脚本，通过 before_stop / advance gate 触发，内部通过 VFS 读取验证 output port 交付。

- 门禁 = 一个 hook_rule，使用与其他 hook 完全相同的基础设施
- Hook Rhai 脚本中调用 VFS read 检查 `lifecycle://artifacts/{port_key}` 是否有内容
- 不做特殊的 "直接查 port_outputs 字段" 捷径——agent 能看到的视图 = 门禁验证的视图
- 如果 VFS read 路径存在循环依赖，说明设计有缺陷，需单独拉 task 修复，不做 workaround
- GateStrategy enum 仍然保留在 OutputPortDefinition 上，但实际检查逻辑由对应的 Rhai preset 实现

### Q5: Input Port 上下文渲染 → 统一到现有 prompt builder 的 locator 解析流程

**决策**：Input port 上下文注入统一到标准的 locator 占位 + 自动解析路径，不新建独立的 context builder。

- Input port 定义在 session build 阶段自动生成 `WorkflowContextBinding` 条目
  - locator = `lifecycle://artifacts/{source_port_key}`（从 edge 的 from_port 推导）
  - reason = 来自 input port 的 description
  - context_strategy / context_template 控制注入方式
- 这些 binding 与 workflow contract 中已有的 context_bindings 合并后，走标准的 `resolve_context_bindings()` 流程
- 不需要新建 `context_builder.rs` 模块——复用现有 prompt builder 路径
- Output port 的交付要求通过 workflow contract 的 instructions 注入（告诉 agent 需要写入哪些 lifecycle://artifacts/{port_key}）

---

## 九、决策对 Gap 的影响修订

基于上述确认的决策，原始 Gap 分析需修订如下：


| 原 Gap                       | 修订                                                                                |
| --------------------------- | --------------------------------------------------------------------------------- |
| A1 (VFS 写入)                 | 路径简化为 `artifacts/{port_key}`，provider 不需要注入 SessionBindingRepository              |
| A2 (门禁检查)                   | 不在 advance_node 中做硬编码检查，改为 hook preset (Rhai) 实现                                  |
| A3 (Orchestrator 上下文注入)     | 不新建 context_builder 模块，orchestrator 在 session build 时生成 context_bindings 条目即可     |
| A4 (session_runtime_inputs) | Input port → 生成 context_bindings → 走标准 resolve_context_bindings 流程                |
| I2 (独立表)                    | 不需要独立表，port_outputs 内嵌在 LifecycleRun JSON 中                                       |
| Phase 1 清单                  | `depends_on` 直接删除而非保留，减少一套兼容分支                                                    |
| Phase 2 清单                  | advance_node.rs 不做门禁硬编码，改为新增 Rhai preset；VFS provider 路径简化                        |
| Phase 3 清单                  | 删除"新文件 context_builder.rs"，改为在 orchestrator / session_runtime_inputs 中生成 bindings |


