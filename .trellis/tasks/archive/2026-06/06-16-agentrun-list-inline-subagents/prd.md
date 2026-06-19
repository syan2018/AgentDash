# PRD — 优化 AgentRun list 前端与子 Agent 内联

## 目标与用户价值

AgentRun 列表（前身 session list）当前重写后出现退化：折叠交互观感差、主会话的关联子 Agent 展示不正确。本任务复活早期 `active-session-list.tsx`（commit `b134b7ab`，577 行版）被验证「妥善」的设计，让列表重新做到：关联子 Agent 眼见为实（含实时状态）、折叠交互轻量对齐、按 subject 分组聚合。

## 背景与确认事实（来自代码勘察）

- 当前列表组件：[active-agent-run-list.tsx](packages/app-web/src/features/agent/active-agent-run-list.tsx)（commit `3346a137` 重写）。
- 侧栏快捷列表：[AgentRunShortcutList.tsx](packages/app-web/src/components/layout/AgentRunShortcutList.tsx)。
- 后端列表接口：[lifecycle_agents.rs](crates/agentdash-api/src/routes/lifecycle_agents.rs) `list`（line ~100）只对 lineage root（主 Run）产出 entry，`subagent_count` = 子树后代总数。
- 子 Agent 当前靠 `get_agent_run_workspace` 懒加载，返回 `AgentRunLineageRef`，该结构（[workflow.rs:1360](crates/agentdash-contracts/src/runtime/workflow.rs#L1360)）**不含 delivery_status / last_activity_at**，故前端子行恒为灰 idle 点、无时间。
- list entry（`AgentRunWorkspaceListEntry`）已带 `subject_ref` + `subject_label`（后端已解析，无需前端 storyStore）。
- 列表投影 `load_agent_run_list_projection` 已产出 shell（含 `delivery_status` / `last_activity_at`）——内联子 Agent 只需对直接子节点复用该投影。
- 早期设计参考：`b134b7ab:active-session-list.tsx`（577 行，`+N` 药丸 + subject 分组 + 内联 subs）、`8d7dd307~1:lifecycle-grouping.ts`（`groupSessionsBySubject`）。
- contracts 经 ts-rs `generate_contracts_ts` bin 生成至 [workflow-contracts.ts](packages/app-web/src/generated/workflow-contracts.ts)；改 Rust 契约后需重新生成。

## 需求

1. **子 Agent 内联到列表接口**：后端 `list` 为每个主 Run entry 内联其**直接（一跳）子 Agent**，每个子 Agent 携带 `delivery_status` / `last_activity_at` / 标题 / `agent_kind` / `agent_role` / `relation_kind` / 自身 `subagent_count`。前端去掉 `fetchAgentRunWorkspace` 懒加载。
2. **折叠交互复活 `+N` 药丸**：去掉强制 chevron 列，顶层行恢复对齐；行内 `+N` chip 点击就地展开/收起直接子 Agent，子行带状态点、角色/关系标签。
3. **复活 subject 分组**：按 list entry 的 `subject_ref` / `subject_label` 将主 Run 分组（如 Story / Task），组头带计数与折叠；无 subject 的归入兜底组。
4. **侧栏快捷列表高度 bug**：[AgentRunShortcutList.tsx](packages/app-web/src/components/layout/AgentRunShortcutList.tsx) 让列表高度跟随内容自适应——条目少时收缩、不留空白，条目多时受可用空间约束并保留「+N 更多」入口。
5. **主列表分页（抗量级）**：`list` 接口支持游标分页（`limit` 默认 30 + 不透明 `cursor`，服务端按 `last_activity_at` 降序 keyset），前端「加载更多」按需续拉，不再一次性拉全量并无限长渲染。投影成本随之降到「一页 Run + 其直接子」。

## 验收标准

- [ ] 主 Run 行展示其直接子 Agent 数量（`+N`），点击就地展开；子行显示**真实** delivery 状态点与相对时间，不再恒为灰 idle。
- [ ] 列表首屏即可见子 Agent 计数，无需逐行点开发起额外请求（无懒加载网络往返）。
- [ ] 顶层行图标/标题对齐，无 chevron 占位空列。
- [ ] 含 subject 的主 Run 按 Story/Task 分组并可折叠，组头显示会话数；无 subject 归入兜底组。
- [ ] 侧栏 AgentRun 快捷列表：条目少时容器高度贴合内容（无大片空白）；条目多时不溢出 sidebar 且保留「更多」入口。
- [ ] 主列表首屏只拉 30 条；底部「加载更多」可续拉下一页（cursor 串接），到尾返回空 `next_cursor` 不再有按钮。
- [ ] 量级大时后端只对当前页 Run + 其直接子跑投影（非全量），首屏不随历史 Run 总量线性变慢。
- [ ] 前端 `pnpm --dir packages/app-web check` 通过；后端 `cargo check` 通过；contracts 已重新生成且无手改漂移。

## Out of Scope

- 任意深度递归树（旧妥善设计仅一层 primary + subs；deeper 子孙通过打开该子 Agent 自身工作台进入，不在列表内递归内联）。
- 右侧工作台 `get_agent_run_workspace` 的 parent/children 链路改造（保留现状供工作台内导航）。
- 列表数据的实时推送（保留现有轮询/拉取节奏）。
- **服务端按状态/关键词过滤**：状态 tab 与搜索在已加载窗口内过滤；服务端过滤需把 `delivery_status` 做成可查字段，留作后续（running/idle 处于最新流顶部，窗口过滤实际影响小）。

## 待澄清（阻塞规划的开放项）

- Q1：一层内联是否可接受（放弃当前列表内任意深度下钻能力）？见 design 决策。
