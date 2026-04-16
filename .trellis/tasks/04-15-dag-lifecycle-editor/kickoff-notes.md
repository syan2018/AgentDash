# DAG Lifecycle Editor 开工说明

## 目的

这份说明用于回答两个问题：

1. **编辑器现在可以依赖哪些后端 / 前端契约？**
2. **当前还有哪些明确未完成的能力边界，不能被前端偷偷当成“已经支持”？**

结论先写在前面：

- **可以正式开始编辑器开发**，至少 Phase 1 / Phase 2（图渲染、node CRUD、edge 连线）已经有稳定的数据契约可依赖
- **不要把当前能力误读成“全量 DAG 运行时都完成了”**
- 本次收口**没有藏兼容垫片**，而是把现有 runtime 真相明确写死，方便编辑器按真相开发

---

## 已收口的前置能力

### 1. LifecycleDefinition 的 CRUD / validate 契约已对齐

当前编辑器可稳定依赖以下事实：

- `LifecycleDefinition` 的 `create / update / validate` 都支持 `edges`
- 前端 draft 在加载 / 校验 / 保存时都会携带 `edges`
- `steps + edges` 是 lifecycle 拓扑的唯一保存真相

这意味着：

- 编辑器做图修改后可以完整 round-trip
- 不会再出现“创建时能带 edges，更新时丢 edges”的情况

### 2. WorkflowContract 的 port 定义可以被前端完整读取

当前前端 service 已可读取：

- `contract.output_ports`
- `contract.input_ports`
- `LifecycleDefinition.edges`
- `WorkflowRun.port_outputs`
- `WorkflowStepState.gate_collision_count`

这意味着：

- side panel 可以直接拿到 port 定义
- runtime overlay 也已经有必要的读取字段

### 3. 后端已经提供编辑器所需的图语义校验

当前应用层新增了以下校验：

- edge 引用的 `from_port / to_port` 必须真实存在于对应 workflow contract
- 同一个 input port 只能接收一条 edge
- output port key 在整个 lifecycle 内必须全局唯一

这三条是编辑器在连线和保存时最重要的后端真相。

---

## 本次明确写死的真相契约

下面这些不是“临时 workaround”，而是当前系统的**正式真相**。编辑器应该直接围绕它们开发。

### A. `port_key` 当前是 **lifecycle 级全局唯一**

当前 runtime 存储与 VFS 访问模型是：

- `LifecycleRun.port_outputs: BTreeMap<String, String>`
- VFS 路径：`lifecycle://artifacts/{port_key}`

这里没有 `node_key` 维度，因此：

- 两个 node 不能各自都定义一个叫 `summary` 的 output port
- 编辑器在新增 / 编辑 output port 时必须把“全局唯一”当成真实约束

这不是兼容包袱，而是当前实现真实能力边界。

### B. `depends_on` 已经退出真相层

编辑器不需要，也不应该，再去读取或生成 `depends_on`。

当前唯一拓扑来源是：

- `steps`
- `edges`

也就是说：

- “线性步骤”只是 `edges` 的一种特殊情况
- 自动生成线性链路时，也应直接生成 `edges`

### C. 编辑器保存的是 **LifecycleDefinition 拓扑**，不是 runtime 执行状态

编辑器当前应只修改：

- `LifecycleDefinition.steps`
- `LifecycleDefinition.edges`
- 以及相关 `WorkflowDefinition.contract.{input_ports, output_ports}`

编辑器当前**不应**尝试写入：

- `LifecycleRun.active_node_keys`
- `LifecycleRun.port_outputs`
- `WorkflowStepState.gate_collision_count`
- 任意 session 级状态

runtime overlay 是只读叠加信息，不是编辑对象。

---

## 这次没有藏的兼容雷

下面这些点特地写出来，是为了避免后续 review 时怀疑“是不是后端偷偷做了某种兼容 fallback”。

### 1. 没有 `camelCase ?? snake_case` 双读兼容

前端 workflow service 只读规范字段，没有为了“先凑合跑”加入双字段兼容。

### 2. 没有保留 `depends_on` 的后门

没有“写 edges、读 depends_on”或“保存时自动回填 depends_on”这种双真相。

### 3. 没有给 output port 做 node-scoped 别名兼容

没有诸如：

- `node_key:port_key`
- `nodes/{node}/artifacts/{port}`
- 平铺路径和 node-scoped 路径双写

这种隐藏兼容层目前都不存在。

当前系统真相就是平铺 `lifecycle://artifacts/{port_key}`。

### 4. 没有把非法 edge 在保存时偷偷修正

如果 edge 引用了不存在的 port，或者一个 input port 接了多条边，后端会直接报校验错误，不会偷偷帮前端改图。

### 5. 没有把 `PhaseNode` 包装成“看起来可编辑、实际上会运行失败”的完整能力

数据模型里有 `node_type = phase_node`，但这不等于 runtime 已完整支持。

换句话说：

- **PhaseNode 在 schema 层存在**
- **PhaseNode 在完整运行语义层还没有收口**

这不是隐藏雷，而是明确的能力边界，见下一节。

---

## 当前仍未完成的能力边界

这些不是编辑器 Phase 1 / 2 的 blocker，但它们是真实存在的边界，前端不能假设已经完成。

### 1. `PhaseNode` 的完整运行语义未闭环

目前还没有完整实现：

- 复用前一个 session
- 切换 workflow contract
- 在同一 session 中继续推进 phase node

因此编辑器第一版建议：

- 可以展示 `PhaseNode`
- 可以保留 node type 字段
- 但**不要默认把它当成“已完全可运行”的能力卖给用户**

更稳妥的做法：

- 第一版 UI 允许显示但附带“运行时待完善”提示
- 或者第一版先只开放 `AgentNode` 创建

### 2. Input Port 的高级上下文策略还没有完整执行链路

当前 runtime 尚未完整落地：

- `Summary`
- `MetadataOnly`
- `Custom`

这些 `ContextStrategy` 的真正执行语义。

所以编辑器第一版建议：

- UI 上可展示枚举，但要标记 `Full` 以外为“预留”
- 或直接只允许编辑 `Full`

### 3. runtime overlay 字段虽然可读，但图上叠加仍属于后续阶段

当前保存 / 校验真相层已足够支撑编辑器开工；
但运行态图叠加（node 动效、port handle 状态、gate collision 可视化）仍建议放到编辑器 Phase 4。

---

## 编辑器首期建议范围

为了避免前端被未闭环运行时拖住，建议首期严格收敛为：

### 推荐直接开工

- React Flow 图渲染
- 从 `LifecycleDefinition.steps + edges` 构图
- node 增删改
- edge 创建 / 删除
- 基于后端校验返回展示错误
- workflow contract 的 input/output port 基础编辑
- 自动线性连线 / auto-layout

### 建议明确降级

- `PhaseNode`：可展示，弱化运行承诺
- `ContextStrategy`：先只支持 `Full`
- `GateStrategy`：先只把 `Existence` 作为可用项，其他显示为预留

---

## 开工前 Checklist

- [x] `LifecycleDefinition` 的 `edges` 可 create / update / validate
- [x] 前端 draft 可 round-trip `edges`
- [x] 前端可读取 workflow contract 的 `input_ports / output_ports`
- [x] 后端可校验 edge → port 引用合法性
- [x] 后端可校验 input port 单来源约束
- [x] 后端可校验 output port key lifecycle 全局唯一
- [ ] `PhaseNode` 完整运行语义
- [ ] `ContextStrategy` 高级模式执行链路

前 6 项已满足，所以编辑器可以正式开始。

---

## 给 Review 的一句话总结

这次收口做的是：

> **把编辑器依赖的数据真相和校验真相补齐，并把未完成的 runtime 能力边界显式写出来。**

没有藏兼容 fallback，也没有偷偷做双真相。

