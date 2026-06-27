# Design — AgentRun list 子 Agent 内联 + 交互/分组复活

## 1. 数据建模决策（核心）

### 现状

- `list`（[lifecycle_agents.rs](crates/agentdash-api/src/routes/lifecycle_agents.rs) line ~118）对每个 run：`list_by_run` 取全部 agent + `agent_lineage_repo.list_by_run` 取全部 lineage 边 → `build_lineage_forest` 在内存建整棵控制树 forest，但**只对 root 产出 entry**，子节点信息丢弃（仅保留 `subagent_count` 数字）。
- 子 Agent 详情只能事后 `get_agent_run_workspace` 懒加载，返回的 `AgentRunLineageRef` 不带 shell 状态。

### 决策：list entry 内联「直接子 Agent」节点，携带真实 shell

forest 已在内存，取直接子节点是零额外查询；唯一新增成本是对每个直接子 Agent 跑一次 `load_agent_run_list_projection`（与 root 同一轻量投影，含 `delivery_status` / `last_activity_at` / 标题）。

- **深度**：仅内联一跳（root 的直接子）。每个子节点仍带自身 `subagent_count` 作为「它下面还有 N 个」的提示；更深子孙不在列表内递归内联（通过打开该子 Agent 工作台进入，见 PRD Out of Scope）。
- **理由**：① 投影成本约束在「根 + 直接子」，列表/侧栏轮询可承受；② 子行获得**真实**状态（优于旧 577 设计——旧版子行复用父级 `executionStatus`，line 519，是已知瑕疵）；③ 与被验证「妥善」的旧设计形状一致（primary + 一层 subs）。
- **被否方案**：递归内联整棵子树 —— 每次轮询对全部后代跑投影，深/宽树成本不可控；收益（列表内深层下钻）已被「打开子工作台」覆盖。

## 2. 契约变更（Rust → ts-rs 重新生成）

文件：[crates/agentdash-contracts/src/runtime/workflow.rs](crates/agentdash-contracts/src/runtime/workflow.rs)

新增子节点类型（专用，不复用 run 级 `AgentRunWorkspaceListEntry`，避免塞入无意义的 run_ref/run_status/subject 字段）：

```rust
/// AgentRun 列表内联的直接子 Agent 节点（携带真实 shell 状态，免懒加载）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub struct AgentRunListChild {
    pub run_ref: LifecycleRunRefDto,
    pub agent_ref: AgentRunRefDto,
    pub agent_kind: String,
    pub agent_role: String,
    pub relation_kind: String,
    pub shell: AgentRunWorkspaceShell, // 含 display_title / delivery_status / last_activity_at
    #[serde(default)]
    pub subagent_count: u32,           // 该子自身子树后代数（深层下钻提示）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery_runtime_ref: Option<RuntimeSessionRefDto>,
}
```

`AgentRunWorkspaceListEntry` 增字段：

```rust
    /// 该主 Run 的直接子 Agent（一跳），已内联 shell 状态，前端免懒加载。
    #[serde(default)]
    pub children: Vec<AgentRunListChild>,
```

`AgentRunWorkspaceListView` 增分页游标（对齐 marketplace `{ items, next_cursor }` 约定）：

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_cursor: Option<String>, // None = 已到尾页
```

重新生成：`pnpm run contracts:generate`（= `cargo run -p agentdash-contracts --bin generate_contracts_ts`），产物 [packages/app-web/src/generated/workflow-contracts.ts](packages/app-web/src/generated/workflow-contracts.ts) 与 [packages/app-web/src/types/lifecycle-views.ts](packages/app-web/src/types/lifecycle-views.ts) 的 re-export。`pnpm run contracts:check` 校验无漂移。

## 3. 后端 list 改造

在 `list` 的 root 循环内（已有 `children_map`）：
- 取 root 的直接子 id：`children_map.get(&root.id)`。
- 对每个直接子 agent：`load_agent_run_list_projection` → 组 `AgentRunListChild`（`subagent_count = count_descendants(child.id, &children_map)`）。
- 填入 entry 的 `children`，按子的 `last_activity_at` 排序。
- 抽出复用函数 `list_child_from_projection(...)`，与 `list_entry_from_projection` 并列。

注意：直接子 agent 来自同一个 `agents`（`list_by_run` 已取全），无需额外 repo 查询；仅投影是新增异步调用。

## 3b. 服务端分页（抗量级核心）

现状问题：`list` 对**全部** run 逐个跑投影后再排序，成本随项目历史 Run 总量线性增长，且每 30s 轮询。分页把这条砍到「一页」。

handler 签名加 `Query`：

```rust
#[derive(Deserialize)]
struct AgentRunListQuery { limit: Option<u32>, cursor: Option<String> }
```

流程改为**先排序取页、再投影**：
1. `lifecycle_run_repo.list_by_project` 取全部 run（仅元数据，无投影）。
2. 按 `run.last_activity_at` 降序、`run.id` 降序 tiebreak 排序。
3. **keyset 游标**：`cursor` 解码为 `(last_activity_at, run_id)`，跳过 ≥ 该键的项；取后续 `limit`（默认 30，clamp 上限如 100）个 run。
4. **仅对页内 run** 取 lineage、建 forest、对 root + 其直接子跑 `load_agent_run_list_projection`，组 entry + children。
5. 若页内取满且后面还有 run，`next_cursor` = 编码最后一项的 `(last_activity_at, run_id)`（base64 不透明串）；否则 `None`。

权衡（见 PRD Out of Scope）：状态/关键词过滤不下推服务端（`delivery_status` 仅在投影后可知），保留前端窗口过滤。排序键用 run 级 `last_activity_at`（与 entry shell 的可能有微小差异，但 run 级是稳定且 pre-projection 可得的键）。

副带优化：侧栏 [AgentRunShortcutList](packages/app-web/src/components/layout/AgentRunShortcutList.tsx) 改为请求 `limit=<可见数+1>` 的首页，而非拉全量（它本就只展示首屏 + 「更多」）。

## 4. 前端列表组件改造

文件：[active-agent-run-list.tsx](packages/app-web/src/features/agent/active-agent-run-list.tsx)

- **分页**：`useState` 累积 `entries` + `nextCursor` + `loadingMore`；首屏 `fetchProjectAgentRuns(projectId, { limit: 30 })`，「加载更多」用 `nextCursor` 续拉并 append；`projectId` 变更时重置。可选：用 IntersectionObserver 滚动到底自动触发。`fetchProjectAgentRuns` service 签名加可选 `{ limit?, cursor? }`。
- 删除懒加载：移除 `fetchAgentRunWorkspace` 调用、`children/loading/error` 局部状态、`MAX_TREE_DEPTH`/`ancestors` 防环（不再递归）。
- `AgentRunTreeRow` → 简化为两级渲染：主行 + 内联子行（不再无限递归组件）。
- **交互换 `+N` 药丸**：参考 `b134b7ab:active-session-list.tsx` 的 `SessionRow`（line 126-137）——主行右侧 `+N` chip（边框/底色/hover），点击 `stopPropagation` 切换本地 `expanded`；去掉左侧强制 chevron 列，顶层行 `StatusDot` 与标题直接对齐。
- 子行：缩进（`pl-7`），`StatusDot` 用子节点真实 `shell.delivery_status`，meta 显示 `agent_role || agent_kind` + `relation_kind`，右侧相对时间用 `shell.last_activity_at`。子节点若 `subagent_count>0` 显示「N sub」只读提示（不在列表内展开）。
- 子行点击 `onOpenAgentRun(child.run_ref.run_id, child.agent_ref.agent_id)`。

## 5. subject 分组复活

- 移植 `8d7dd307~1:lifecycle-grouping.ts` 的 `groupSessionsBySubject`，但**数据源换为 list entry 的 `subject_ref` / `subject_label`**（后端已解析标签，删除对 storyStore/findStoryById 的依赖）。
- 分组键 `subject_ref.kind:subject_ref.id`，标签 `subject_label`；无 `subject_ref` 归入兜底组（label「项目会话」或「其他」）。
- 组头组件参考 577 版 `SubjectGroupHeader`（折叠箭头 + kind 徽标 + label + 计数）。
- 仅当存在 >1 组或唯一组非兜底时展示分组（`hasGroups`），否则平铺，沿用旧逻辑。
- 分组 + 子 Agent 展开两套折叠状态各自独立（`collapsedGroups` / `expandedSubAgents`），与 577 版一致。

## 6. 侧栏快捷列表高度 bug

文件：[AgentRunShortcutList.tsx](packages/app-web/src/components/layout/AgentRunShortcutList.tsx)

- 根因：外层 `flex min-h-0 flex-1`（line 163）恒抢满 sidebar 剩余高度；`listRef` 又挂在该节点测 `clientHeight` → 容器永远填满，条目少时留空白。
- 修法：拆两层职责——
  - 外层「测量包络」：保留 `flex-1 min-h-0`，挂 `listRef` + ResizeObserver 仅用于**测可用高度**算 `maxVisible`，自身不直接承载内容高度。
  - 内层「内容列表」：高度由内容决定（不 `flex-grow`），渲染 `visibleEntries`；条目少时自然收缩贴合内容，条目多时受外层可用高度约束 + 「+N 更多」入口（沿用现有 overflow 逻辑）。
- 即：让内容块 `h-auto` 在测量包络内顶对齐，而非 `flex-1` 撑满。验证条目 1/3/20 三档下无空白、无溢出、「更多」入口正确出现。

## 7. 影响面与兼容

- `get_agent_run_workspace` parent/children 链路与 `AgentRunLineageRef` 保持不变（工作台内导航仍用），仅列表侧新增内联。
- contracts 新增字段均 `#[serde(default)]`，旧前端读取不破坏；前端类型重新生成后引用新字段。
- 测试：`useAgentRunWorkspaceState.test.ts` 等若 mock list view 需补 `children: []` 默认。

## 8. 验证

- 后端：`cargo check -p agentdash-api -p agentdash-contracts`。
- 契约漂移：重新生成后 `git diff` 仅含预期字段，无手改。
- 前端：`pnpm --dir packages/app-web check`（typecheck + lint + test）。
- 手验：列表首屏可见 `+N`、展开子行有真实状态色；分组折叠；侧栏高度三档正常。
