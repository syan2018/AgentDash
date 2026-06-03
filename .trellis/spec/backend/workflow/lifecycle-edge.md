# Workflow Lifecycle Edge

Lifecycle DAG edge 约束 Activity 之间的控制流与数据依赖。模块不变量见 [Workflow Architecture](./architecture.md)。

## Edge Kinds

Lifecycle edge 有且仅有两种 kind：

| Kind | 语义 | Port 字段 |
| --- | --- | --- |
| `flow` | 控制流：前驱完成即激活后继 | 不携带 `from_port` / `to_port` |
| `artifact` | 数据依赖：端口级产出/消费 | 必须同时声明 `from_port` + `to_port` |

定义在 `agentdash-domain/src/workflow/value_objects.rs`。`LifecycleEdge` 持有 `kind`、`from_node`、`to_node`、`from_port`、`to_port`。

Lifecycle edge 应通过 `LifecycleEdge::flow()` / `LifecycleEdge::artifact()` 构造，避免调用点绕过 kind/port 约束。

## Artifact Implies Flow

`artifact: A.out -> B.in` 自动等价于 node-level flow dependency `A -> B`。

`node_deps_from_edges()` 把 flow/artifact 两类边统一聚合为 node 级依赖。只有在有顺序要求但无数据产出时，才显式声明 flow edge。

## Runtime Advancement

1. `WorkflowGraph` 按 `entry_activity_key` 初始化 root graph instance。
2. Activity attempt 完成后，遍历 edges 找出 dependency set 已满足的 pending 后继并标为 Ready。
3. 所有 Activity 进入终态时，lifecycle 置 `Completed`；active display 由 graph instance attempts 派生。
4. 无出边的 Activity 是 terminal，不需要 `is_terminal` 字段。

多 Activity lifecycle 必须显式声明 edges；运行时不按 activities 数组顺序线性推进。

## Validation

`validate_lifecycle_definition` 按顺序执行以下校验，任一失败即拒绝：

| 规则 | 错误信息片段 |
| --- | --- |
| `activities.len() >= 2 && edges.is_empty()` | `workflow_graph.edges 不能为空` |
| Entry Activity 不可有入边 | `entry_activity_key 不应有入边` |
| Entry Activity 必须是 Agent Activity | `入口 Activity 必须是 Agent Activity` |
| 禁止孤岛 Activity（单 Activity lifecycle 除外） | `workflow_graph.activities X 是孤岛` |
| `kind=flow` 不可有 port | `edges[i] kind=flow 不应携带 port` |
| `kind=artifact` 必须有 port | `edges[i] kind=artifact 必须同时声明 from_port 与 to_port` |
| 禁止自连接 | `edges[i] 不能自连接` |
| 禁止循环依赖 | `lifecycle DAG 存在循环依赖` |

单 Activity lifecycle 允许无 edges；该 Activity 同时是 entry 和 terminal。

## JSON Contract

- `kind` 字段必写。
- `from_port` / `to_port` 为 `Option<String>`，`None` 时不序列化。
- 所有 builtin JSON 模板必须显式声明 `kind` 字段。

示例：

```json
{ "kind": "flow", "from_node": "plan", "to_node": "apply" }
{ "kind": "artifact", "from_node": "research", "from_port": "report", "to_node": "implement", "to_port": "research_input" }
```

## Frontend Rendering

Lifecycle DAG editor 按 kind 分派 edge 样式：

- Flow edge：实线 + primary 色；连接创建点为 node body。
- Artifact edge：虚线 + border 色 + port 标签；必须连接到 port handle。

连接判定：handle 非空且非 `__default_*` 占位 -> artifact；否则 flow。
