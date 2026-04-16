# DAG Lifecycle 可视化编辑器

## Goal

将前端的 lifecycle 编辑体验从当前 drawer 内的平铺列表升级为完整的 DAG 可视化编辑器，支持 node 拖拽布局、port-level 连线、side panel 节点配置，以及与 I/O port 体系的深度集成。

## Background

当前 lifecycle 的编辑 UI 仅在 drawer 中以线性列表形式展示 step 定义，信息密度不足以支撑 DAG 拓扑的编辑：

1. **无法表达依赖关系** — 列表形式无法直观展示 node 间的 edge 连接和并行分支
2. **无法管理 port 连线** — 引入 I/O port 后，每条 edge 连接特定的 output port 到 input port，需要可视化操作
3. **无法预览 DAG 结构** — 用户无法直观感知整个 lifecycle 的拓扑形态
4. **节点配置空间不足** — drawer 中无法容纳 workflow 选择、port 配置、上下文模板等丰富配置

## Architecture Decisions

### AD1: 混合编辑方案

- **图区域（React Flow）**：负责 DAG 拓扑的可视化预览和连线操作
  - Node 以卡片形式渲染，展示 node key、类型标签、port 列表
  - 拖拽 node 调整布局（auto-layout + 手动微调）
  - 从 output port handle 拖拽到 input port handle 创建 edge
  - 选中 node 在 side panel 展开配置
- **Side Panel（表单）**：负责选中 node 的详细配置
  - Node 基础信息（key, description, node_type）
  - Workflow 选择（从已有 workflow definitions 中选择或创建）
  - Output port 管理（增删改，门禁策略配置）
  - Input port 管理（增删改，上下文构建策略配置）
  - Input port 的上下文模板编辑（预设策略或自定义模板）

### AD2: 图编辑与定义持久化

- 图的布局信息（node position）存储在 LifecycleDefinition 的扩展字段或独立存储
- 拓扑结构（nodes, edges, ports）持久化到 LifecycleDefinition 的 steps + edges 字段
- 编辑器操作产生 draft 状态，用户显式保存后同步到后端
- 支持从现有线性 lifecycle 自动转换为 DAG 图布局

### AD3: 页面集成位置

- 在现有 workflow 管理区域新增 "Lifecycle Editor" 入口
- 编辑器作为独立的全宽面板组件，不再局限于 drawer
- 从 lifecycle 列表点击进入编辑器，编辑完成后返回列表

## Requirements

### R1: React Flow DAG 图区域

- 使用 `@xyflow/react` 渲染 DAG 拓扑
- Node 自定义渲染：
  - Header：node key + node type badge（Agent Node / Phase Node）
  - Body：workflow name（如已关联）、port 数量摘要
  - Left handles：input ports（Target type）
  - Right handles：output ports（Source type）
  - 选中态：高亮边框 + side panel 展开
- Edge 自定义渲染：
  - 从 source node 的 output port handle 连接到 target node 的 input port handle
  - Edge label 显示连接的 port 名称
  - 可选择 edge 进行删除
- 控制功能：
  - Minimap 导航
  - Zoom controls
  - Auto-layout 按钮（dagre 布局算法）
  - 添加 node 按钮（选择 Agent Node 或 Phase Node）

### R2: Side Panel 节点配置

- **基础信息 Tab**
  - Node key（唯一标识，创建后不可改）
  - Description（自由文本）
  - Node type（Agent Node / Phase Node 切换）
  - Workflow 关联（下拉选择已有 WorkflowDefinition，或留空）
- **Output Ports Tab**（仅 Agent Node 显示）
  - Port 列表（key, description）
  - 每个 port 的门禁策略选择：
    - Existence（文件存在即通过）
    - Schema（预留，配置 content_type / required_fields）
    - LLM Judge（预留，配置 evaluation_prompt）
  - 添加 / 删除 port
- **Input Ports Tab**（仅 Agent Node 显示）
  - Port 列表（key, description）
  - 每个 port 的上下文构建策略选择：
    - Full（默认，完整内容注入）
    - Summary（仅注入摘要）
    - Metadata Only（仅注入元信息）
    - Custom（显示 prompt 模板编辑器）
  - 自定义模板编辑时，提供变量提示（`{{artifact.content}}`、`{{artifact.title}}` 等）
  - 添加 / 删除 port

### R3: Edge 管理

- 从 output port handle 拖拽到 input port handle 创建 edge
- 验证：
  - 不允许自连接（同一 node 内的 port 互连）
  - 不允许环（DAG 约束）
  - Input port 只能接收一条 edge（单一数据源）
  - Output port 可连接多个 input port（fan-out）
- 选中 edge 后可删除
- Edge 渲染显示 port 名称

### R4: 持久化与保存

- 编辑器内所有操作产生 local draft 状态
- "保存" 按钮将 draft 同步到后端：
  - 更新 LifecycleDefinition 的 steps、edges
  - 如果 port 定义变化，同步更新对应 WorkflowDefinition 的 contract
- "重置" 按钮恢复到最后保存的状态
- 离开页面前如有未保存更改，提示确认

### R5: 运行时状态叠加

- 当 lifecycle 有活跃 run 时，图上叠加运行时状态：
  - Node 背景色/边框色反映 step 状态（Pending/Ready/Running/Completed/Failed）
  - Output port handle 颜色反映交付状态（未写入/已写入/门禁通过/门禁失败）
  - Gate collision 计数显示在 node 上
  - Active node 脉动动画
- 运行时状态叠加为只读，不影响编辑操作

### R6: 便捷线性 Edge 生成

- 项目未上线，不存在旧数据迁移需求。`depends_on` 已直接移除，edges 是唯一拓扑数据源
- 编辑器提供 "按当前 steps 顺序自动生成线性 edges" 的便捷操作（类似 auto-wire）
- Auto-layout 将线性 topology 渲染为从左到右的链式布局
- 用户可在此基础上添加并行分支、调整拓扑

## Acceptance Criteria

- React Flow 图正确渲染 DAG 拓扑，node 和 edge 可交互
- Node 上正确显示 input/output port handles
- 从 output port handle 拖拽到 input port handle 可创建 edge
- Side panel 支持 node 基础信息、output port、input port 的编辑
- Output port 可配置门禁策略（existence 首期可用）
- Input port 可选择上下文构建策略，Custom 策略可编辑 prompt 模板
- 保存操作正确持久化到后端 LifecycleDefinition + WorkflowDefinition
- 运行时状态正确叠加到 DAG 图上
- 编辑器支持按 steps 顺序自动生成线性 edges 的便捷操作
- Auto-layout 功能正常（dagre 算法）
- 环检测和 edge 验证规则正确执行

## Implementation Phases

### Phase 1: 基础图渲染 + Node CRUD

- React Flow 集成和自定义 Node 组件
- 从 LifecycleDefinition 数据加载图
- 添加/删除 node
- Auto-layout

### Phase 2: Port Handle + Edge 连线

- 自定义 Port handle 组件
- Edge 创建和验证（DAG 约束、端口约束）
- Edge 渲染和删除

### Phase 3: Side Panel 配置面板

- Node 基础信息表单
- Output port 管理 + 门禁策略配置
- Input port 管理 + 上下文策略配置
- Prompt 模板编辑器

### Phase 4: 持久化 + 运行时叠加

- Draft 状态管理 + 保存/重置
- 后端 API 调用
- 运行时状态叠加渲染
- 线性 edge 自动生成便捷操作

## Technical Notes

- 前端依赖 `@xyflow/react` 已在项目 package.json 中（需确认版本）
- Auto-layout 推荐使用 `@dagrejs/dagre` 或 `elkjs`
- Node position 数据如果不存入 LifecycleDefinition，需要独立的 layout storage（可以用 localStorage 做初版）
- Port handle 的视觉设计需要与现有 AgentDash UI 风格统一（rounded corners, subtle borders, muted colors）
- 大型 DAG（>20 nodes）的性能需关注 React Flow 的 virtualization 配置

## Dependencies

- **前置**：`04-15-agent-node-io-port-gating` — 提供 port/edge 数据模型和 VFS 写入能力
- **关联**：`04-13-lifecycle-dag-orchestration` — 已完成 DAG 编排基础框架

## 开工说明

- 编辑器开工前的后端 / 前端真相层收口与能力边界说明，见 [`kickoff-notes.md`](./kickoff-notes.md)
- 该说明明确了当前**可直接依赖的保存/校验契约**，以及**不能假设已完成**的运行时能力（如 `PhaseNode` 完整语义、`ContextStrategy` 高级模式）

## Related Tasks

- `04-15-agent-node-io-port-gating` — 数据模型前置依赖
- `04-13-lifecycle-dag-orchestration` — DAG 编排已实现
- `04-13-local-dashboard-ui` — 前端 UI 体系
