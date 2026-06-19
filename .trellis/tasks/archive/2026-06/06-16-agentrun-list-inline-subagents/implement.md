# Implement — AgentRun list 子 Agent 内联 + 交互/分组复活

## 顺序检查表

### A. 后端契约（先行，驱动前端类型）
- [ ] A1. [workflow.rs](crates/agentdash-contracts/src/runtime/workflow.rs) 新增 `AgentRunListChild` 结构（run_ref / agent_ref / agent_kind / agent_role / relation_kind / shell / subagent_count / delivery_runtime_ref?），derive `Serialize, Deserialize, TS`，`rename_all = "snake_case"`。
- [ ] A2. `AgentRunWorkspaceListEntry` 增 `#[serde(default)] pub children: Vec<AgentRunListChild>`；`AgentRunWorkspaceListView` 增 `#[serde(default, skip_serializing_if=Option::is_none)] #[ts(optional)] pub next_cursor: Option<String>`。
- [ ] A3. `pnpm run contracts:generate` 重新生成；确认 [workflow-contracts.ts](packages/app-web/src/generated/workflow-contracts.ts) 出现新类型/字段；[lifecycle-views.ts](packages/app-web/src/types/lifecycle-views.ts) re-export 增 `AgentRunListChild`。

### B. 后端 list 内联 + 分页（[lifecycle_agents.rs](crates/agentdash-api/src/routes/lifecycle_agents.rs)）
- [ ] B1. 新增 `list_child_from_projection(run, projection, relation_kind, agent_kind, subagent_count) -> AgentRunListChild`（与 `list_entry_from_projection` 并列，复用 `shell_model_to_contract`）。
- [ ] B2. handler 加 `Query<AgentRunListQuery>{ limit: Option<u32>, cursor: Option<String> }`。流程改「先排序取页、再投影」：`list_by_project` → 按 `run.last_activity_at` desc + `run.id` desc 排序 → 解码 cursor 做 keyset skip → 取 `limit`（默认 30，clamp ≤100）个 run。
- [ ] B3. **仅对页内 run** 取 lineage/建 forest/跑投影：root entry + 其直接子（`children_map.get(&agent.id)` → 对应 `LifecycleAgent` + lineage `relation_kind` → `load_agent_run_list_projection` → `AgentRunListChild`，`subagent_count = count_descendants`）。children 按 `last_activity_at` 降序。
- [ ] B4. cursor 编解码 helper：`(last_activity_at, run_id)` ↔ base64 不透明串；页满且有余则置 `next_cursor`，否则 `None`。
- [ ] B5. 补单测：① entry.children 含直接子且带真实 delivery_status；② 分页 limit/cursor keyset 正确（取页、next_cursor、尾页为 None）。参考已有 `#[cfg(test)]` 模块（line ~1176）。

### C. 前端列表组件（[active-agent-run-list.tsx](packages/app-web/src/features/agent/active-agent-run-list.tsx)）
- [ ] C0. service [lifecycle.ts](packages/app-web/src/services/lifecycle.ts) `fetchProjectAgentRuns(projectId, opts?: { limit?: number; cursor?: string })` 透传 query；返回含 `next_cursor`。
- [ ] C1. 删除懒加载：移除 `fetchAgentRunWorkspace` import/调用、`children/loading/error` state、`MAX_TREE_DEPTH`、`EMPTY_ANCESTORS`、`ancestors` 防环、`ExpandChevron`。
- [ ] C1b. 分页 state：累积 `entries` + `nextCursor` + `loadingMore`；首屏 limit=30；底部「加载更多」按 `nextCursor` 续拉 append；`projectId` 变更重置。可选 IntersectionObserver 自动触发。
- [ ] C2. 主行：去掉左侧强制 chevron 列；`StatusDot` + 标题直接对齐（参考 `b134b7ab:active-session-list.tsx` SessionRow）。
- [ ] C3. `+N` 药丸：主行右侧 chip（`role=button` + `stopPropagation`），切本地 `expanded`；展开渲染 `entry.children` 子行。
- [ ] C4. 子行：缩进 `pl-7`，`StatusDot` 用 `child.shell.delivery_status` 真实状态；meta = `agent_role || agent_kind` + `relation_kind`；时间 = `child.shell.last_activity_at`；`child.subagent_count>0` 显示「N sub」只读提示。点击 `onOpenAgentRun(child.run_ref.run_id, child.agent_ref.agent_id)`。
- [ ] C5. 搜索过滤同时匹配子 Agent 标题（可选，沿用顶层匹配即可）。

### D. subject 分组
- [ ] D1. 新建 `packages/app-web/src/features/agent/agent-run-grouping.ts`：`groupAgentRunsBySubject(entries)` 按 `subject_ref.kind:id` 分组，label 用 `subject_label`，无 subject 入兜底组（移植自 `8d7dd307~1:lifecycle-grouping.ts`，去掉 storyStore 依赖）。
- [ ] D2. 列表渲染：`hasGroups` 时渲染 `SubjectGroupHeader`（折叠箭头 + kind 徽标 + label + 计数），组内复用 C 的行渲染；否则平铺。`collapsedGroups` 与 `expandedSubAgents` 两套独立 state。

### E. 侧栏高度 bug（[AgentRunShortcutList.tsx](packages/app-web/src/components/layout/AgentRunShortcutList.tsx)）
- [ ] E1. 拆层：外层保留 `flex-1 min-h-0` 作「测量包络」挂 `listRef`/ResizeObserver 算 `maxVisible`；内层内容列表改为内容高（不 `flex-grow`，顶对齐），条目少时收缩。
- [ ] E2. 副带：侧栏拉取改 `limit=<maxVisible+1>` 首页（不再全量），「更多」入口跳全列表页。
- [ ] E3. 验证 1 / 3 / 20 条三档：少时无空白、多时不溢出且「+N 更多」入口正确。

### F. 收尾
- [ ] F1. 修复受影响测试（如 mock list view 需补 `children: []`）。
- [ ] F2. `pnpm run contracts:check` 无漂移。
- [ ] F3. `cargo check -p agentdash-api -p agentdash-contracts` + 后端单测。
- [ ] F4. `pnpm --dir packages/app-web check`。

## 验证命令

```bash
pnpm run contracts:check
cargo check -p agentdash-api -p agentdash-contracts
cargo test -p agentdash-api lifecycle_agents
pnpm --dir packages/app-web check
```

## 风险点 / 回滚

- 风险：直接子投影增加 list 接口耗时（root 数 × 直接子数 次投影）。缓解：仅一跳；若实测过慢，可改为子节点用更轻的 meta（不跑完整 list 投影）作为后续优化。
- 回滚点：契约新增字段均 `#[serde(default)]`，前端改动集中在 active-agent-run-list.tsx / 新 grouping 文件 / AgentRunShortcutList.tsx，可单文件 revert。
- B 步与 C 步可分别独立验证（先后端 entry.children 有数据，再前端消费）。
