# Lifecycle DAG 化与多 Session 编排架构演进

## 背景

当前 lifecycle 系统是严格线性的步骤序列，且与 agent session 存在 1:1 的隐式绑定。随着对 lifecycle 功能定位的深入讨论，明确了三个核心演进方向：

1. **Lifecycle 作为上下文容器** — 不仅是阶段锚点和合约切换器，更是跨 session 的有效上下文积累与传递载体
2. **Lifecycle DAG 化** — 从线性步骤序列演进为有向图，支持条件分支和 join，类似 n8n 等 workflow 引擎
3. **Session 与 Lifecycle 解耦** — lifecycle run 不再与单个 agent session 绑定，而是作为多 session 的编排容器

## 核心定位

Lifecycle 在本项目中的完整职能定义为三层：

| 层 | 职责 | 当前状态 |
|---|---|---|
| **阶段锚点** | 告诉系统"当前在哪一步" | 已实现 |
| **合约切换** | 根据阶段加载不同的 hook/injection/completion | 已实现 |
| **上下文容器** | 积累、组织、暴露跨 session 的有效上下文，支撑多 agent 接力与 subagent 上下文访问 | 骨架有，闭环未打通 |

## 设计讨论记录

### Session 模型演进（重要）

详细讨论见 [`session-model-evolution.md`](session-model-evolution.md)。核心结论：

**Lifecycle session ≠ agent session。** Lifecycle session 是 lifecycle run 的前端观测容器，本身不绑定 agent。每个 agent node 创建自己的独立 agent session。不存在一个持续在场的 "host conductor agent"。

lifecycle 的编排由 `LifecycleOrchestrator` 服务驱动（通过 session terminal 事件 + agent 主动 tool call），不需要一个 agent 来充当 conductor。

用户和哪个 session 交互完全是前端层面的能力选择——底层所有 session 都完全支持输入。

### Node 类型

| 类型 | 行为 | 在 DAG 中的位置 |
|---|---|---|
| **Agent Node** | 创建独立 agent session 执行工作 | DAG 一级 node |
| **Phase Node** | 不创建新 session，在前一个 session 内切换 workflow contract | DAG 一级 node |

两者都是 DAG 拓扑中的一级节点。连续的 phase node 共享同一个 session。

### Node 与 Session 的关系

- Agent node: 1 node → 1 新 session
- Phase node: N 个连续 phase node → 共享 1 个 session（由前一个 agent node 创建）
- Fan-out 并行是 node 内部的 subagent 行为，不是工作流层面的真并行

### Agent 与 Lifecycle 的交互接口

- **Node/Phase 推进**：Agent 通过 tool call 主动声明完成。这是唯一推进路径（D2）
- **Artifact 沉淀**：通过现有 `report_workflow_artifact` tool 或扩展
- **上下文查询**：通过 lifecycle VFS mount 读取前驱 node 产出
- **门禁拦截**：不正确推进状态时，session 不允许结束（before_stop hook）

具体 tool 接口待设计。

## 已知耦合点

按改造难度排列（详细分析见讨论记录）：

### 硬骨头

| 耦合点 | 位置 | 问题 |
|---|---|---|
| `Task.session_id: Option<String>` | Task domain entity | 一个 task 只能绑一个 session |
| Lifecycle Run 查找路径 | `projection.rs` `resolve_active_workflow_projection` | 始终从 `(binding_kind, owner_id)` 查 run，无 session → run 直接映射 |
| Hook snapshot 单 workflow | `hooks/provider.rs` `SessionSnapshotMetadata.active_workflow` | `Option<T>` 单数，completion 评估也是单 workflow |
| `current_step_key` 单数 | `entity.rs` `LifecycleRun` | DAG 下多个 node 可同时 active |

### 中等难度

| 耦合点 | 位置 | 问题 |
|---|---|---|
| Session Plan 片段生成 | `session/plan.rs` | 假设单一 owner_type + 单一 phase |
| Bootstrap Plan | `session/bootstrap.rs` | `workflow: Option<ActiveWorkflowProjection>` 单数 |
| Hook advance 机制 | `session/hook_runtime.rs` | `pending_advance` 是 `Option`，一次只推一个 step |
| 前端 UI | `task-workflow-panel.tsx` | 假设 1 task : 1 run : 1 session |

### 容易改的

| 耦合点 | 位置 | 问题 |
|---|---|---|
| Address Space 挂载 | `session_runtime_inputs.rs` | mount ID 硬编码为 `"lifecycle"` |
| SessionBinding 模型 | `session_binding/entity.rs` | 本身已支持 N:M，不阻塞 |

## 上下文容器未闭环的 Gap

1. **Node 间动态产出无法自动注入** — 前驱 node 沉淀的 artifact 不会出现在后继 node 的 `context_bindings` 中，agent 只能手动通过 VFS 查阅
2. **Artifact 可寻址性不够** — 只能按 UUID 寻址，缺少 by-type / by-node 的查询路径（lifecycle-vfs-typed-access task 在跟进）

解决方案（已决策 D3）：让 `context_bindings` 支持 lifecycle VFS 内部 locator：
```json
{
  "locator": "lifecycle://nodes/research/artifacts/by-type/session_summary",
  "reason": "research 阶段的产出摘要",
  "required": true
}
```

## 建议演进路径

### Phase 0 — 解耦前置（纯重构，不改变外部行为）

- [ ] 从 Task entity 移除 `session_id: Option<String>`，统一走 SessionBinding
- [ ] 给 LifecycleRun 增加 node → session_id 的映射结构
- [ ] `resolve_active_workflow_projection` 支持直接传入 `run_id + node_key`
- [ ] 优化 agent 操作 lifecycle 的工具接口

### Phase 1 — 线性 DAG + 双类型 Node（功能增量）

- [ ] Node model 替代 step model（V1 保持线性拓扑，不开放分支）
- [ ] Agent node: 创建独立 session，完成后 orchestrator 推进
- [ ] Phase node: 在前一个 session 内切换 workflow contract
- [ ] 上下文容器通过 VFS locator 引用前驱 node 的 artifact
- [ ] Agent 主动 tool call 推进 + before_stop 门禁拦截

### Phase 2 — DAG 拓扑 + 条件分支

- [ ] 引入 edges with conditions
- [ ] All-complete join 实现（已决策 D4）
- [ ] 前端 DAG 可视化编辑器
- [ ] LifecycleOrchestrator 服务：DAG 状态维护 + 门禁验证（已决策 D2）
- [ ] Agent 主动推进 + 门禁注入的完整闭环

## 已决策

### D1: Node 类型 → 双类型 Node（Agent Node + Phase Node）

DAG 中有两种一级 node 类型：

- **Agent Node** — 创建独立 agent session 执行工作。
- **Phase Node** — 不创建新 session，在前一个 session 内切换 workflow contract。连续的 phase node 共享同一个 session。

两者都是 DAG 拓扑中的一级节点，phase node 不是 agent node 的内部概念。

不存在一个持续在场的 "host conductor agent"。没有 conductor 不代表只有 agent node——phase node 是 DAG 中合法的节点类型，用于"多 node 共享一个 session"的场景。

### D2: DAG 编排驱动方式 → Agent 主动推进 + 系统门禁

**唯一推进路径：Agent 主动 tool call → Orchestrator 验证门禁 → 放行或拒绝。**

不留 "session terminal 也能推进" 之类的模糊退路。如果 agent 没有正确推进状态，session 就不允许结束。

具体机制：
- **独立 `LifecycleOrchestrator` 服务**负责维护 DAG 状态、评估 node 可达性、管理门禁规则
- **系统通过 hook injection 自动注入门禁信息**，告诉 agent "你当前在哪个 node、完成条件是什么、下一步可以去哪"
- **Agent 通过 tool call 主动声明 node/phase 完成**，orchestrator 验证门禁条件
- **门禁不通过 → session 不允许结束**（before_stop hook 拦截），agent 必须满足条件后重试

这保证了单一事实来源：推进状态只有 agent 主动调用这一条路径。

### D3: 上下文传递 → 引用式（VFS locator）

采用消费方声明式引用：

- 后继 node 的 `WorkflowContract.injection.context_bindings` 通过 `lifecycle://` locator 引用前驱 node 的产出
- 不在 DAG edge 上定义传递规则，由消费方（后继 node 的 workflow contract）声明需要什么
- 与现有 VFS 架构最契合，不引入新的传递抽象层

示例：
```json
{
  "locator": "lifecycle://nodes/research/artifacts/by-type/session_summary",
  "reason": "research 阶段的产出摘要",
  "required": true
}
```

### D4: 并行分支 Join → All-complete

当 DAG 拓扑中存在并行分支汇聚时，采用 **all-complete join** 语义：所有前驱 node 必须完成后，后继 node 才被激活。

注意：工作流层面不做真 fan-out 并行，需要并发执行的场景交给 node 内部的 subagent 机制处理。DAG 的并行分支更多是逻辑拓扑上的"不同维度的工作可独立推进"。

### D5: Companion 与 Lifecycle 的关系 → 独立机制，互不侵入

**Agent node session 的创建和上下文注入走标准 session 创建流程，不走 companion。**

- Orchestrator 直接通过 SessionHub 创建 agent node session
- 上下文通过 workflow contract 的 `context_bindings`（含 lifecycle VFS locator）注入
- 结果回收由 Orchestrator 监听 session terminal 事件完成
- Lifecycle 身份标识走 `SessionBinding(label="lifecycle_node:{key}")` + `LifecycleRun.node_state.session_id`
- **不扩展 CompanionSessionContext**，不增加 lifecycle 字段，不污染 companion 数据结构
- Companion 保持原样：仅用于 agent node session 内部的 subagent dispatch

详细分析见 [`companion-reuse-analysis.md`](companion-reuse-analysis.md)。

### D6: 前端渲染 → 页面级 LifecycleSessionView

Lifecycle session 是 lifecycle run 的一级视图（非某个 agent session 的消息流嵌套）：

- 独立的页面级组件 `LifecycleSessionView`，不依赖 `SessionChatView`
- 顶部：lifecycle 进度条（Phase 1 线性，Phase 2 DAG 可视化）
- 主体：agent node 卡片列表，每个 node 可展开为该 session 的完整消息流（复用 `AcpSessionList`）
- 活跃 node 默认展开 + 提供输入框
- 各 node session 事件通过独立 `useAcpStream` 实例获取（展开时连接，折叠时断开）

详细设计见 [`frontend-session-rendering.md`](frontend-session-rendering.md)。

### D7: Lifecycle Session 模型 → lifecycle run 的前端视图

Lifecycle session 不是后端实体，也不是某个 agent session，而是 lifecycle run 的前端视图：

- 不引入 "LifecycleSession" 实体，不引入 "host session" / "lifecycle_conductor" 概念
- 各 agent node session 通过 `SessionBinding(label="lifecycle_node:{node_key}")` 标识归属
- 在 `LifecycleRun.node_state` 上记录 `session_id`，建立 node → session 的关联
- Lifecycle session 的生命周期跟 lifecycle run 对齐，与任何 agent session 无关

详细讨论见 [`session-model-evolution.md`](session-model-evolution.md)。

### D8: 所有推进（node 和 phase）→ Agent 主动 tool call

Agent 通过 tool call 主动声明当前 node/phase 完成。这是 D2 "唯一推进路径" 原则的具体体现。

- Phase node 完成 → agent 调 tool → 切换 workflow contract，session 继续
- Agent node 完成 → agent 调 tool → orchestrator 验证门禁 → session 可以结束 → orchestrator 启动后继 node

不存在 "session terminal 自动推进" 的隐式路径。agent 不正确推进就不让结束。

### D9: Agent Node 执行 → 全部阻塞式

Agent node 执行期间 lifecycle 是阻塞等待的。Orchestrator 等 session terminal 事件后才评估下一个 node 的可达性。一次只有一个 node 在执行（DAG 并行分支除外）。

**原因**：lifecycle DAG 是逻辑编排，保证严格的步骤顺序。真正的并发执行交给 node 内部的 subagent 机制。

### D10: 用户和子 session 的交互 → 纯前端层面决策

底层所有 agent session 完全支持用户输入。是否为某个 session 提供输入框是前端渲染层的选择：

- 当前活跃的 agent node session → 默认提供输入框
- 已完成的 session → 默认只读，可选择"继续对话"
- 未启动的 node → 不可交互

不需要在后端区分"可交互"和"只读"。

## 待决策

- [ ] Agent node 的 tool 接口设计（advance_lifecycle_node / report_artifact / 上下文查询）
- [ ] Agent node 在数据模型中的具体表达（LifecycleNodeDefinition 结构）
- [ ] 门禁注入的具体格式（markdown？structured JSON？混合？）
- [ ] Lifecycle VFS locator 中 `lifecycle://nodes/{key}/...` 的完整路径语法
- [ ] Lifecycle session 的前端入口和路由设计

## 关联任务

- `03-30-lifecycle-vfs-typed-access` — Lifecycle VFS 按类型访问语法扩展
- `04-01-agentdash-gsd-workflow-alignment` — GSD 工作流体验复刻规划
- `03-30-session-context-mount` — SessionContextSnapshot Mount 化

## 状态

**Planning** — 架构讨论阶段，不进入实现。
