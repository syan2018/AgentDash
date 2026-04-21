# Workflow Lifecycle Edge 语义重构（flow + artifact 双维度）

## Goal

当前 `LifecycleEdge` 只有 **port 级 artifact 依赖** 一种形态，却同时承担"控制流推进"和"数据传递"两个职责。对于没有产出 port 的 step（或非 artifact 驱动的顺序约束），运行时只能靠 [entity.rs:335-340](crates/agentdash-domain/src/workflow/entity.rs#L335-L340) 的 **fallback 线性推进** 兜底——导致 [builtin_workflow_admin.json](crates/agentdash-application/src/workflow/builtins/builtin_workflow_admin.json) 这种 lifecycle 前端 DAG 预览为空但后端"偷偷能跑"的语义割裂。

**目标**：引入 **flow edge（控制流）** 与 **artifact edge（数据依赖）** 双维度，去除 fallback 兜底，所有 lifecycle 通过显式 edges 驱动执行；前端 DAG 与后端运行时对齐。

## What I already know

### 代码现状

- **Edge 模型**：[value_objects.rs:389-397](crates/agentdash-domain/src/workflow/value_objects.rs#L389-L397) 只有一种 `LifecycleEdge { from_node, from_port, to_node, to_port }`
- **Node 依赖推导**：[value_objects.rs:665-674](crates/agentdash-domain/src/workflow/value_objects.rs#L665-L674) 的 `node_deps_from_edges()` 把 port edge 聚合为 node 依赖
- **Fallback 分支**：[entity.rs:331-344](crates/agentdash-domain/src/workflow/entity.rs#L331-L344) `complete_step` 中 `has_edges==false` 时按 step 数组顺序线性推进
- **现存 builtin**：
  - `builtin_workflow_admin.json`：**无 edges**，依赖 fallback
  - `trellis_dag_task.json`：有 edges（line 103 起），正常 DAG
- **前端编辑器**：[lifecycle-dag-editor.tsx:97-109](frontend/src/features/workflow/lifecycle-dag-editor.tsx#L97-L109) `lifecycleEdgesToRfEdges`、[line 274-318](frontend/src/features/workflow/lifecycle-dag-editor.tsx#L274-L318) 连接创建——全部以 port edge 为模型
- **Step port 定义**：[value_objects.rs:408-413](crates/agentdash-domain/src/workflow/value_objects.rs#L408-L413) step 有 `output_ports` / `input_ports` 字段（已可选）
- **Migration**：当前最高 0016。上次动过 edge 表结构是 0009 `lifecycle_edges_port_outputs.sql`。下一个编号 **0017**

### 关键约束

- `LifecycleEdge` 类型通过 `schemars::JsonSchema` 暴露给前端（TypeScript 生成）——schema 变动会同步 ripple 到前端类型
- `validate_edge_topology` 已有 entry node 不应有入边、禁止自环等规则
- lifecycle 运行时状态机通过 `step_states[].status` + `active_node_keys` 表达激活集合

## Assumptions (temporary)

- 存量生产数据有限（这是内嵌系统，没外部租户），可以一次性 migration 补齐 edges
- 分离后 **artifact edge 隐含 flow 约束**（"B 消费 A 的 port → B 必在 A 之后"）更符合直觉；flow edge 只补"无 artifact 但有顺序要求"的空隙
- 本轮 **不引入 condition / fork-join / loop 等复杂控制流语义**——仅做最朴素的"完成后激活后继"

## Research Notes

### 行业惯例

| 系统 | 控制流 / 数据流 分离 | 备注 |
|---|---|---|
| **Airflow** | 显式分离：`>>` 表示控制依赖，TaskFlow 的 `XCom` 走数据 | DAG 关系和 XCom 关系可以不一致 |
| **Prefect 2.x** | `wait_for=` 控制依赖；参数传递走数据 | flow 参数自动建立数据依赖 |
| **Temporal** | 活动调用隐含顺序，数据走参数传递 | 用代码而非 DAG 表达依赖 |
| **Argo Workflows** | `depends:` 字段表达控制流；artifact 单独定义 | 明确两种维度 |
| **n8n / Node-RED** | 连线合一（控制+数据） | 和当前系统类似，低代码友好但语义混乱 |

**共识**：严肃的 workflow 引擎基本都走向分离；**artifact 依赖自动贡献 flow 约束** 是常见做法（避免冗余声明）。

### 三种可行形态

#### Approach A：单 edge 类型 + `kind` 字段（推荐）

```rust
pub enum LifecycleEdgeKind { Flow, Artifact }

pub struct LifecycleEdge {
    pub kind: LifecycleEdgeKind,
    pub from_node: String,
    pub to_node: String,
    // 仅 Artifact kind 使用
    pub from_port: Option<String>,
    pub to_port: Option<String>,
}
```

- **How**：单一集合，kind 区分语义；artifact 必须带 port；flow 不带 port
- **Pros**：改动最小；校验集中；前端一份 edges 两种渲染（颜色/虚实线）；未来扩展 `Trigger`/`Compensation` 直接加枚举值
- **Cons**：`Option<String>` port 在 serde/schema 上不够"类型安全"；编辑器要做 kind 分派

#### Approach B：两个独立集合

```rust
pub struct LifecycleDefinition {
    pub flow_edges: Vec<FlowEdge>,
    pub artifact_edges: Vec<ArtifactEdge>,
}
```

- **How**：schema 层面硬分离；两种 edge 类型各司其职
- **Pros**：类型最安全；语义最清晰
- **Cons**：Schema / migration / 编辑器 / 校验都翻倍；前端要合并两个集合才能渲染；未来加第三种 edge 还要再开一个字段

#### Approach C：保留单 edge，port 设为可选（不推荐）

- **How**：`from_port: Option<String>`，不区分 kind；有 port 就是 artifact，没 port 就是 flow
- **Pros**：改动最小
- **Cons**：语义混乱；未来无法区分 `Trigger`/`Compensation`；只是把"fallback"从运行时挪到数据层

### 倾向

**Approach A**——在改动代价与语义清晰度之间最平衡，且对未来扩展（condition、compensation、trigger）友好。

## Open Questions

（全部决策已收敛，见 Decision 节）

## Decision (ADR-lite)

### D1：Schema 形态 — Approach A（单 edge + kind 字段）

**Context**：Edge 当前只有 port 级 artifact 一种形态，要引入 flow/artifact 双维度。

**Decision**：采用 Approach A：`LifecycleEdge { kind: Flow | Artifact, from_node, to_node, from_port: Option, to_port: Option }`。

**Consequences**：
- 改动集中在单一 schema；校验分派靠 match `kind`
- port 变 `Option<String>`——需要在 validate 阶段强约束（flow 不可有 port / artifact 必须有 port）
- 前端 edges 仍是单集合，渲染时按 kind 分派样式
- 未来扩展 `Trigger` / `Compensation` 只需加枚举值，不需要新字段

### D2：Artifact 隐含 Flow 约束

**Context**：声明 `artifact: A.out → B.in` 时，B 是否自动等 A 完成？

**Decision**：**artifact edge 自动贡献 flow 依赖**——就绪判定时把 flow/artifact 两类 edge 的 from_node 合并成 dependency set。

**Consequences**：
- 用户只在"无产出但需顺序"的情景显式补 flow edge
- 对齐 Airflow TaskFlow / Prefect 参数传递的行业惯例
- 失去"data 传了但不阻塞流程"的表达（本轮不需要；未来可通过额外 flag 扩展）

### D3：控制流扩展（condition / fork-join）本轮不做、预留空间

**Context**：flow edge 一旦独立出来，自然会联想到条件分支、并行汇聚策略。

**Decision**：本轮仅实现"无条件前驱完成即激活"语义，**默认 AND-join**（多入边必须全部完成）。不实现 condition / fork 显式节点；**不预置死字段**——下一轮按需加。拆独立任务 `04-21-workflow-lifecycle-branching-design` 专门讨论。

**Consequences**：
- schema 保持简洁，`LifecycleEdgeKind` 枚举后续可安全扩展
- 下一轮可能引入的字段：`condition` 表达式、`join_policy` 节点字段、甚至 `kind: Trigger` / `kind: Compensation`
- **设计方向锚点（供下一轮参考）**：condition 大概率不会是孤立 DSL，而是要**暴露给 agent 工作流工具**或与 **hook check 流程集成**——条件判定本质上是"agent/hook 给个信号"而非纯表达式求值。这会影响 schema 形态（可能是 `condition: { hook_key: String }` 或 `{ tool_call: ... }` 而非 `condition: "expr"`）

### D4：存量数据一次性 migration，运行时彻底去 fallback

**Context**：去掉 fallback 后，DB 中既有"无 edges"lifecycle 会回归为"启动即 Completed 空跑"。

**Decision**：
- Migration **0017** 扫 `lifecycle_definitions`，对 `edges=[]` 且 `steps.len() ≥ 2` 的记录按 steps 数组顺序生成线性 flow edges
- Builtin JSON（`builtin_workflow_admin.json`）手工补齐 edges
- 运行时**彻底移除** [entity.rs:335-340](crates/agentdash-domain/src/workflow/entity.rs#L335-L340) 的 fallback 分支；`validate_lifecycle_definition` 对 `steps.len() ≥ 2 && edges.is_empty()` 直接报错
- 无兼容窗口、无 warning 软迁移

**Consequences**：
- 一次性干净；运行时语义单一
- Migration 需要处理新 `kind` 枚举的 JSON 序列化（flow edges 写入时 `kind: "flow"`, from_port/to_port 缺省）
- 升级后任何新建 lifecycle 必须显式声明 edges，编辑器需保证不产出无 edges 的多 step lifecycle

### D5：Terminal 隐式判定 + 禁止孤岛

**Context**：fallback 移除后，"lifecycle 何时完成"必须由 schema 明确。

**Decision**：
- **Terminal 隐式**：无出向 flow/artifact edge 的 step 即视为终点
- Lifecycle 完成条件：所有"可达"的 step 都处于终态（Completed / Skipped），且 `active_node_keys` 空
- 新增校验规则：
  - 禁止**孤岛 step**（既无入边也无出边）——单 step lifecycle 除外（此时该 step 既是 entry 又是 terminal）
  - `steps.len() ≥ 2 && edges.is_empty()` 直接报错
  - Flow edge 不可携带 port；Artifact edge 必须携带 port
  - Entry step 不可有入边（既有规则保留）
- 不加 `is_terminal` 字段，不加 `NodeType::Terminal` 变体——保持最小改动

**Consequences**：
- 无额外字段；符合 DAG 直觉
- 单 step lifecycle 需要特殊放行（否则会被"禁孤岛"误判）
- 未来若需"多终点 + 任一到达即完成"语义，再加字段即可；本轮不需要

## Requirements (evolving)

- **Domain**：
  - `LifecycleEdge` 新增 `kind: LifecycleEdgeKind` 字段；`from_port` / `to_port` 变 `Option<String>`
  - `LifecycleEdgeKind` 枚举：`Flow` / `Artifact`（serde `snake_case`）
  - `node_deps_from_edges` 保持签名——flow + artifact 两类都贡献 node 依赖
- **Runtime**：
  - 彻底移除 [entity.rs:335-340](crates/agentdash-domain/src/workflow/entity.rs#L335-L340) 的 fallback 线性推进分支
  - `complete_step` 无 edges 场景仅对"单 step lifecycle"放行（此时该 step 即 entry 即 terminal）
- **Validation**（`validate_lifecycle_definition` / `validate_edge_topology`）：
  - `steps.len() ≥ 2 && edges.is_empty()` → 报错
  - Flow edge 不可有 port；Artifact edge 必须有 port
  - 禁止孤岛 step（无入边且无出边），单 step lifecycle 除外
  - Entry step 不可有入边（既有规则保留）
- **Migration 0017**：
  - 扫 `lifecycle_definitions` 表，对 `edges=[]` 且 `steps.len() ≥ 2` 的记录按 steps 数组顺序补线性 flow edges（`kind: "flow"`, port 缺省）
  - 既有 artifact edges 一律补 `kind: "artifact"` 字段
- **Builtin**：
  - `builtin_workflow_admin.json` 补 `{kind: "flow", from_node: "plan", to_node: "apply"}`
  - `trellis_dag_task.json` 既有 edges 补 `kind: "artifact"` 字段
- **前端**：
  - [lifecycle-dag-editor.tsx](frontend/src/features/workflow/lifecycle-dag-editor.tsx) edge 样式按 kind 分派：flow = 实线、artifact = 虚线（颜色待定）
  - 连接创建逻辑：source/target handle 对应 port → artifact；无 port 的 node 连接 → flow
  - 编辑器不允许保存"多 step 无 edges"lifecycle（前端校验 + 后端兜底）
- **单测覆盖**：
  - Rust：纯 flow / 纯 artifact / 混合三种 lifecycle 的 `complete_step` 推进路径
  - Rust：arfifact 隐含 flow 依赖（仅 artifact edge 能正确激活后继）
  - Rust：校验规则（孤岛禁止、flow 不可有 port、artifact 必须有 port、多 step 无 edges 报错）
  - 前端：编辑器连接创建时 kind 分派正确

## Acceptance Criteria (evolving)

- [ ] `LifecycleEdge` 支持 `Flow` / `Artifact` 两种 kind
- [ ] `complete_step` 移除 fallback；无 edges 的多 step lifecycle validate 直接失败
- [ ] `builtin_workflow_admin` 运行后 lifecycle 能正确 `plan → apply → Completed`
- [ ] 前端 DAG 预览 `builtin_workflow_admin` 能看到 plan → apply 的 flow 连线（区分于 artifact）
- [ ] Migration 0017 本地回放：既有无 edges lifecycle 自动补线性 flow edges，无 warning
- [ ] Rust unit test：三种推进场景 + 校验规则 全绿
- [ ] 前端编辑器能创建两种 kind 的 edge 并区分渲染
- [ ] 单 step lifecycle（仅 1 个 step、无 edges）不被误判为孤岛

## Definition of Done

- `cargo test -p agentdash-domain -p agentdash-application -p agentdash-infrastructure` 绿
- 前端 `pnpm typecheck` / `pnpm build` 绿
- Migration 0017 本地回放无 warning，能正确处理既有两个 builtin 的导入
- `builtin_workflow_admin` 手工运行一遍，观测前端 DAG 有连线、后端状态机正确推进
- Spec 文档更新（`.trellis/spec/` 下添加 lifecycle edge kind 的契约说明）

## Out of Scope (explicit)

- **Condition edge**（条件分支 `if/else`）
- **Fork/Join 显式语义**（并行与汇聚——当前 DAG 自然支持，但不引入专门字段）
- **Compensation / Rollback edge**（事务补偿）
- **Trigger edge**（外部事件触发）
- Lifecycle 版本化 / 迁移链路的自动升级
- Artifact edge 的 port 数据格式校验（schema 层面的 port 类型系统）

## Technical Approach

1. **Domain 改动**（`agentdash-domain`）：
   - `LifecycleEdgeKind { Flow, Artifact }` 枚举；`LifecycleEdge` 加 `kind` + port 改 Option
   - `validate_edge_topology` 扩展：kind 感知 port 约束、孤岛禁止
   - `node_deps_from_edges` 无需改动（flow/artifact 都贡献依赖）
2. **Runtime 改动**（`agentdash-domain::entity`）：
   - 移除 `complete_step` 中 fallback 分支（335-340 行）
   - 单 step lifecycle 在 `complete_step` 完成 entry step 后直接置 Completed
3. **Migration 0017**（`agentdash-infrastructure/migrations`）：
   - SQL UPDATE：`lifecycle_definitions` 中 `edges` jsonb 字段扫描补齐
   - 对 `edges=[]` 且 steps ≥ 2：按 steps 数组顺序生成 `[{kind:"flow", from_node:s[0].key, to_node:s[1].key}, ...]`
   - 对已有 edges：`jsonb_set` 每个 edge 补 `kind:"artifact"` 字段
4. **Builtin 修复**：`builtin_workflow_admin.json` 补一条 `{kind:"flow", from:"plan", to:"apply"}`；`trellis_dag_task.json` 既有 edges 补 `kind:"artifact"`
5. **前端**（`frontend/src/features/workflow/`）：
   - Edge 类型生成后带 `kind` 字段（JsonSchema 自动传播）
   - `lifecycleEdgesToRfEdges` 按 kind 映射 RF edge 的 style / strokeDasharray
   - `onConnect` 分派：连接发生在 port handle 之间 → artifact；在 node body 之间 → flow
6. **Spec 文档**：`.trellis/spec/backend/` 或 `shared/` 下新增 `workflow-lifecycle-edge.md`，记录 kind 语义、artifact 隐含 flow 的规则、校验约束

## Implementation Plan (small PRs)

- **PR1（Domain + Validation）**：
  - `LifecycleEdgeKind` 枚举 + `LifecycleEdge` 结构改动
  - `validate_lifecycle_definition` / `validate_edge_topology` 新规则
  - 单测：三种推进场景 + 校验规则
  - 移除 `complete_step` fallback
  - **交付点**：`cargo test -p agentdash-domain` 绿；老代码调用点会编译失败 → 驱动 PR2
- **PR2（Infra + Migration + Builtin）**：
  - Migration 0017
  - `builtin_workflow_admin.json` / `trellis_dag_task.json` 补 edges + kind
  - Application / Infrastructure 调用 `LifecycleEdge` 构造的地方适配新字段
  - **交付点**：`cargo test --workspace` 绿；本地启动能导入 builtin、migration 回放干净
- **PR3（前端 + 端到端验证）**：
  - 前端 edge 渲染 kind 分派 + 连接创建逻辑
  - 前端编辑器"无 edges 多 step"禁止保存
  - 手工验证：`builtin_workflow_admin` 前端 DAG 有连线、后端运行推进正常
  - Spec 文档落地
  - **交付点**：`pnpm typecheck`/`pnpm build` 绿 + 端到端可跑

## Technical Notes

- Edge schema 上次变动见 migration 0009，可参考格式
- `schemars::JsonSchema` 会自动传播到前端 TS 类型，前端类型改动是"被动"发生的
- [entity.rs:182-237](crates/agentdash-domain/src/workflow/entity.rs#L182-L237) `LifecycleRun::new` 里通过 entry_step_key 初始化 active_node_keys，这块逻辑在 flow edge 模型下仍可复用
- [value_objects.rs:553-555](crates/agentdash-domain/src/workflow/value_objects.rs#L553-L555) `validate_edge_topology` 需要扩展支持 kind 感知校验（例如：flow edge 不应有 port；artifact edge 必须有 port）
