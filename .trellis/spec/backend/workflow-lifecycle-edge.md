# Workflow Lifecycle Edge 设计

> 约束 lifecycle DAG 中 edge 的语义与校验规则。与 [domain-payload-typing.md](./domain-payload-typing.md) / [database-guidelines.md](./database-guidelines.md) 一起参考。

---

## 1. 两种 Edge Kind

Lifecycle 的 edge 有且仅有两种 kind：

| Kind | 语义 | Port 字段 |
|------|------|-----------|
| `flow` | 控制流：前驱完成即激活后继 | **不可** 携带 `from_port` / `to_port` |
| `artifact` | 数据依赖：端口级产出/消费 | **必须** 同时声明 `from_port` + `to_port` |

### 1.1 Rust 类型

```rust
// crates/agentdash-domain/src/workflow/value_objects.rs
pub enum LifecycleEdgeKind { Flow, Artifact }

pub struct LifecycleEdge {
    pub kind: LifecycleEdgeKind,
    pub from_node: String,
    pub to_node: String,
    pub from_port: Option<String>,  // 仅 artifact 使用
    pub to_port: Option<String>,    // 仅 artifact 使用
}

impl LifecycleEdge {
    pub fn flow(from: impl Into<String>, to: impl Into<String>) -> Self { ... }
    pub fn artifact(from: impl Into<String>, from_port: ..., to: ..., to_port: ...) -> Self { ... }
}
```

**强制使用构造器**：禁止通过 struct 字面量构造 edge（除非在特殊测试中显式构造非法 edge 用于校验断言）。构造器保证 kind/port 约束一致。

---

## 2. Artifact 隐含 Flow 约束

**核心规则**：`artifact: A.out → B.in` **自动等价于** flow edge `A → B`。

```rust
// node_deps_from_edges() 把 flow/artifact 两类边统一聚合为 node 级依赖
pub fn node_deps_from_edges(edges: &[LifecycleEdge]) -> HashMap<&str, BTreeSet<&str>> {
    let mut deps = HashMap::new();
    for edge in edges {
        deps.entry(edge.to_node.as_str())
            .or_default()
            .insert(edge.from_node.as_str());
    }
    deps
}
```

**应用场景**：

- 你声明 `artifact: plan.design → apply.design_input`，**不需要** 再加 `flow: plan → apply`
- 只在"有顺序要求但无数据产出"时才显式加 flow edge（例如预热 step、清理 step）

---

## 3. 运行时推进规则

1. `LifecycleRun::new` 按 `entry_step_key` 初始化活跃集合
2. `complete_step` 完成某 step 后，遍历 edges 找出所有"dependency set 已满足"的 pending 后继 → 标为 Ready
3. `active_node_keys` 空 + 所有 step 进入终态（Completed/Skipped）→ lifecycle 置 `Completed`
4. **无出边的 step 即 terminal**（隐式判定，不需要 `is_terminal` 字段）

**关键变更（2026-04-21）**：运行时**不再** fallback 到"按 steps 数组顺序线性推进"。任何多 step lifecycle 必须显式声明 edges。

---

## 4. 校验规则（`validate_lifecycle_definition`）

按顺序执行以下校验，任一失败即拒绝：

| 规则 | 错误信息片段 |
|------|---------------|
| `steps.len() >= 2 && edges.is_empty()` → 拒绝 | `lifecycle.edges 不能为空` |
| Entry step 不可有入边 | `entry_step_key 不应有入边` |
| 禁止孤岛 step（无入边也无出边；单 step lifecycle 除外） | `lifecycle.steps X 是孤岛` |
| `kind=flow` 不可有 port | `edges[i] kind=flow 不应携带 port` |
| `kind=artifact` 必须有 port | `edges[i] kind=artifact 必须同时声明 from_port 与 to_port` |
| 禁止自连接（`from_node == to_node`） | `edges[i] 不能自连接` |
| 禁止循环依赖（Kahn's algorithm） | `lifecycle DAG 存在循环依赖` |

**单 step lifecycle** 是特例：允许无 edges；该 step 同时是 entry 和 terminal。

---

## 5. JSON 序列化约定

- `kind` 字段 **必写**；存量数据无 `kind` 时 serde 默认解析为 `artifact`（兼容历史 port-based edge 数据）
- `from_port` / `to_port` 为 `Option<String>`，`None` 时不序列化（`skip_serializing_if`）
- 所有 builtin JSON 模板都**必须**显式声明 `kind` 字段（便于阅读和维护）

示例：

```json
{ "kind": "flow", "from_node": "plan", "to_node": "apply" }
{ "kind": "artifact", "from_node": "research", "from_port": "report",
  "to_node": "implement", "to_port": "research_input" }
```

---

## 6. 前端渲染约定

[lifecycle-dag-editor.tsx](../../frontend/src/features/workflow/lifecycle-dag-editor.tsx) 按 kind 分派 ReactFlow edge 样式：

- **Flow edge**：实线 + primary 色；连接创建点为 node body（无 handle）
- **Artifact edge**：虚线 (`strokeDasharray: "6 4"`) + border 色 + port 标签；必须连接到 port handle

**连接判定**：handle 非空且非 `__default_*` 占位 → artifact；否则 flow。

---

## 7. Migration 路径（历史资产升级）

参考 `0017_lifecycle_edge_kind.sql`：

- 既有 artifact edge 数据补 `"kind": "artifact"` 字段
- `edges=[]` 且 `steps.len() >= 2` 的 lifecycle 按 steps 数组顺序补线性 flow edges（恢复原 fallback 语义）
- 单 step lifecycle 不处理

新增 lifecycle 时，**不允许** 依赖此类数据修补——必须在定义时就声明完整 edges。

---

## 8. 未来扩展（预留）

以下暂未实现，但 `LifecycleEdgeKind` 枚举保留了扩展空间（见任务 `04-21-workflow-lifecycle-branching-design`）：

- **Condition edge**：条件分支（倾向于用 hook/agent tool 信号而非孤立 DSL 表达式）
- **Fork/Join policy**：并行汇聚语义（`All` / `Any` / `N-of-M`）
- **Trigger edge**：外部事件触发
- **Compensation edge**：事务补偿

**非目标**：Loop / cyclic DAG（需要重新设计运行时状态机）。