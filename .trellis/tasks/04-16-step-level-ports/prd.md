# Port 归属迁移：Workflow Contract → Step 级 + 拖拽自动创建

## Goal

将 I/O port 的归属从 `WorkflowDefinition.contract` 迁移到 `LifecycleStepDefinition`，使 lifecycle step 成为产出契约的 owner，workflow 退回为纯行为策略模板。同时在 DAG 编辑器中实现"拖拽 output port 到节点 body 自动创建同名 input port"的交互。

**动机**：当前 port 绑死在 workflow contract 上，导致同一 workflow 在不同 lifecycle 场景中需要不同产出时必须创建重复的 workflow definition。将 port 上移到 step 后：
- Workflow = 可复用的行为策略（hooks + injection + constraints + completion）
- Step = 具体的 DAG 节点，定义该节点在此 lifecycle 中的输入输出契约

## What I already know

### 当前数据模型

```
WorkflowDefinition.contract
  ├── injection / hooks / constraints / completion   ← 行为约束（保留）
  ├── output_ports: Vec<OutputPortDefinition>        ← 要迁移到 step
  └── input_ports: Vec<InputPortDefinition>          ← 要迁移到 step

LifecycleStepDefinition
  ├── key / description / workflow_key / node_type   ← 当前字段
  └── (无 ports)                                     ← 要新增
```

### 当前 port 读取链路（需全部改为读 step）

| 用途 | 位置 | 读取源 |
|------|------|--------|
| 门禁检查 | `advance_node.rs:140-239` | `workflow.contract.output_ports` |
| Output 上下文注入 | `orchestrator.rs:381-406` | `contract.output_ports` |
| Input 上下文注入 | `orchestrator.rs:408-447` | `contract.input_ports` + edges |
| Hook snapshot | `hooks/provider.rs:358-375` | `contract.output_ports` |
| Session writable keys | `session_runtime_inputs.rs:77-86` | `contract.output_ports` |
| Lifecycle mount | `mount.rs:398-435` | contract ports |
| Edge→port 校验 | `catalog.rs:360-364` | workflow.contract ports |
| Port key 全局唯一 | `catalog.rs:170-188` | workflow.contract ports |
| Port 定义校验 | `value_objects.rs:593-627` | WorkflowContract |
| VFS read/write | `provider_lifecycle.rs:202-293` | run.port_outputs (不变) |

### 不变的约束

- `port_key` 在整个 lifecycle 内全局唯一（校验位置改为读 step ports）
- VFS 路径 `lifecycle://artifacts/{port_key}` 保持平铺
- `LifecycleRun.port_outputs: BTreeMap<String, String>` 存储结构不变

### 前端现状

- DAG 编辑器已有 `portOverrides: Map<stepKey, {inputPorts, outputPorts}>` — 这就是 step-level ports 的雏形
- 旧 workflow-editor 无 port 编辑 UI
- 前端 service 层的 port 映射在 `mapWorkflowContract` 中

## Requirements

### R1: 后端 — LifecycleStepDefinition 新增 ports 字段

```rust
pub struct LifecycleStepDefinition {
    pub key: String,
    pub description: String,
    pub workflow_key: Option<String>,
    pub node_type: Option<LifecycleNodeType>,
    pub output_ports: Vec<OutputPortDefinition>,  // 新增
    pub input_ports: Vec<InputPortDefinition>,    // 新增
}
```

- `OutputPortDefinition` / `InputPortDefinition` struct 不变，只是归属从 contract 移到 step
- 新增字段默认为空 `Vec`，对现有无 port 的 step 无影响

### R2: 后端 — WorkflowContract ports 改为 recommended（可选）

```rust
pub struct WorkflowContract {
    pub injection: ...,
    pub hook_rules: ...,
    pub constraints: ...,
    pub completion: ...,
    // 原 output_ports / input_ports 改为 recommended，仅模板用途
    pub recommended_output_ports: Vec<OutputPortDefinition>,
    pub recommended_input_ports: Vec<InputPortDefinition>,
}
```

- 这些字段仅供编辑器"导入推荐 ports"功能使用
- Runtime 代码不再读取 recommended_ports

### R3: 后端 — Runtime 读取源全部切到 step

上表列出的 7 个读取点全部从 `workflow.contract.output_ports` 改为 `step.output_ports`：

1. `advance_node.rs` — 门禁检查读 step.output_ports
2. `orchestrator.rs` — 上下文注入读 step.output_ports / step.input_ports
3. `hooks/provider.rs` — hook snapshot 读 step.output_ports
4. `session_runtime_inputs.rs` — writable keys 读 step.output_ports
5. `mount.rs` — lifecycle mount 读 step ports
6. `catalog.rs` — edge→port 校验读 step ports
7. `catalog.rs` — port key 全局唯一校验读所有 steps 的 ports

### R4: 后端 — API 序列化

Lifecycle CRUD 接口的 steps 字段现在包含 ports，JSON 透传即可（serde derive）。Workflow CRUD 保持 contract 的 `recommended_*` 字段。

### R5: 前端 — TS 类型更新

```typescript
export interface LifecycleStepDefinition {
  key: string;
  description: string;
  workflow_key?: string | null;
  node_type?: LifecycleNodeType;
  output_ports: OutputPortDefinition[];   // 新增
  input_ports: InputPortDefinition[];     // 新增
}
```

### R6: 前端 — DAG 编辑器移除 portOverrides

- `portOverrides` local state 删除
- Node 的 port 数据直接从 `draft.steps[i].output_ports / input_ports` 读取
- Side Panel 的 port 编辑直接写 `updateLifecycleStep(idx, { output_ports: [...] })`
- Store 的 `addLifecycleStep` 初始化空 ports: `output_ports: [], input_ports: []`

### R7: 前端 — 拖拽自动创建 input port

当用户从 node A 的 output port handle 拖拽到 node B 的 body（而非某个具体 input port handle）时：

1. 在 node B 的 `input_ports` 中自动创建一个同名 port：`{ key: sourcePortKey, description: "" }`
2. 创建 edge：`{ from_node: A.key, from_port: sourcePortKey, to_node: B.key, to_port: sourcePortKey }`
3. 如果 node B 已有同名 input port，直接连线不重复创建

实现方式：利用 `__default_in` handle 作为 "node body drop zone"。`onConnect` 检测 `targetHandle === "__default_in"` 时触发自动创建逻辑。

### R8: 前端 — 导入 Workflow 推荐 Ports

在 Side Panel 绑定 workflow 时，如果该 workflow 有 `recommended_output_ports` / `recommended_input_ports`：
- 显示提示："该 Workflow 推荐以下 ports，是否导入？"
- 用户确认后合并到 step 的 ports 中（跳过已存在的同名 port）

## Acceptance Criteria

- [ ] `LifecycleStepDefinition` 含 `output_ports` / `input_ports`，可正常 CRUD
- [ ] Runtime 门禁检查、上下文注入、hook snapshot 全部从 step ports 读取
- [ ] Edge→port 校验从 step ports 读取
- [ ] Port key lifecycle 全局唯一校验从所有 steps 的 ports 汇总
- [ ] WorkflowContract 的 ports 字段改为 recommended（不影响 runtime）
- [ ] 前端 DAG 编辑器直接编辑 step ports，无 portOverrides
- [ ] 拖拽 output port 到节点 body 自动创建同名 input port + edge
- [ ] 绑定 workflow 时可导入推荐 ports
- [ ] 现有无 port 的 lifecycle 正常工作（向后兼容）

## Definition of Done

- typecheck / lint / build 通过
- 后端编译无 warning（port 相关）
- 前端 DAG 编辑器可正常操作 step-level ports
- 现有 lifecycle 数据兼容（空 ports = 无产出约束）

## Out of Scope

- `ContextStrategy` 高级模式（Summary / MetadataOnly / Custom）的执行链路
- `GateStrategy` 高级模式（Schema / LlmJudge）的执行链路
- PhaseNode 完整运行语义
- Port 跨 lifecycle 复用/引用
- 数据迁移脚本（项目未上线，无历史数据）

## Technical Notes

### 后端影响面盘点

**需修改文件（按依赖顺序）：**

1. `crates/agentdash-domain/src/workflow/value_objects.rs`
   - `LifecycleStepDefinition` 新增字段
   - `WorkflowContract` ports → recommended_ports
   - Port validation 逻辑调整

2. `crates/agentdash-application/src/workflow/catalog.rs`
   - Edge→port 校验改读 step.ports
   - Port key 全局唯一校验改读 all steps' ports

3. `crates/agentdash-application/src/workflow/orchestrator.rs`
   - Output/input context injection 改读 step.ports

4. `crates/agentdash-application/src/workflow/tools/advance_node.rs`
   - Gate check 改读 step.output_ports

5. `crates/agentdash-application/src/hooks/provider.rs`
   - Hook snapshot 改读 step.output_ports

6. `crates/agentdash-application/src/task/session_runtime_inputs.rs`
   - Writable keys 改读 step.output_ports

7. `crates/agentdash-application/src/address_space/mount.rs`
   - Lifecycle mount 改读 step ports

### 前端影响面

1. `frontend/src/types/workflow.ts` — LifecycleStepDefinition + WorkflowContract
2. `frontend/src/services/workflow.ts` — lifecycle service 新增 port 映射
3. `frontend/src/stores/workflowStore.ts` — LifecycleEditorDraft、addLifecycleStep
4. `frontend/src/features/workflow/lifecycle-dag-editor.tsx` — 移除 portOverrides、改读 step ports
5. `frontend/src/features/workflow/ui/dag-side-panel.tsx` — port 编辑写 step
6. `frontend/src/features/workflow/ui/dag-node.tsx` — port 数据源变更

### 兼容性

- 项目未上线，不需要数据迁移脚本
- WorkflowContract 的 `recommended_*` 字段 serde default 为空 Vec
- LifecycleStepDefinition 新增字段 serde default 为空 Vec
- 现有 lifecycle 的 steps 没有 ports → 无产出约束，正常运行

## Implementation Phases

### Phase 1: 后端数据模型 + 校验迁移

- LifecycleStepDefinition 新增 ports
- WorkflowContract ports → recommended_ports（含 serde alias 兼容）
- Catalog 校验逻辑切到读 step ports
- 编译通过

### Phase 2: 后端 Runtime 切换

- orchestrator / advance_node / hooks / session_runtime_inputs / mount 全部切到读 step ports
- 编译通过 + 基础功能测试

### Phase 3: 前端类型 + Service + Store

- TS 类型更新
- Service 映射新增 step port 字段
- Store draft 初始化含空 ports
- DAG 编辑器移除 portOverrides，直接读写 step ports

### Phase 4: 拖拽自动创建 + 导入推荐

- onConnect 检测 __default_in → 自动创建 input port
- 绑定 workflow 时提示导入 recommended ports
