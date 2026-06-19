# 执行计划：AgentRun 列表收束与会话身份建模

按"domain → 后端查询/契约 → 契约生成 → 前端"顺序推进。每个 checkpoint 后跑对应验证。

## Step 0 — 探查复用点（research）
- [ ] git 历史 / 归档任务（`02-26-frontend-agent-session-mvp`、`06-11-agentrun-workspace-frontend-route-state`）确认早期 session 列表自适应高度 hook 是否仍可复用。
- [ ] 确认 `LifecycleSubjectAssociation` 在 workspace view 里能否拿到 task/story 可读标题（subject_label 来源 [lifecycle_agents.rs:802](../../../crates/agentdash-api/src/routes/lifecycle_agents.rs)）。
- 验证：无需编译，记录结论到本文件或 research/。

## Step 1 — domain: agent_role 常量与 builder
- [ ] [lifecycle_agent.rs](../../../crates/agentdash-domain/src/workflow/lifecycle_agent.rs) 新增 `agent_role` 常量模块 + `with_role` / `new_child`。
- 验证：`cargo build -p agentdash-domain`。
- 回滚点：仅 domain，独立可 revert。

## Step 2 — 创建路径写入真实 role
- [ ] [dispatch_service.rs](../../../crates/agentdash-application/src/workflow/dispatch_service.rs) `create_agent`：parent 存在时按 `relation_kind` 写 role；与 lineage 同路径。
- [ ] [gate_control.rs](../../../crates/agentdash-application/src/companion/gate_control.rs) companion child 写 `COMPANION`。
- 验证：`cargo build -p agentdash-application`；新增/调整针对性单测（旧低价值测试可改）。

## Step 3 — 后端列表收束
- [ ] 契约 [workflow.rs](../../../crates/agentdash-contracts/src/runtime/workflow.rs) `AgentRunWorkspaceListEntry` 增 `subagent_count` / `agent_role`。
- [ ] [lifecycle_agents.rs:97-150](../../../crates/agentdash-api/src/routes/lifecycle_agents.rs) `get_project_agent_runs`：run 内内存建 forest，root = 无父节点的 agent，仅 root 产出 entry；`subagent_count` = 子树传递闭包后代数（DFS + visited 防环 + 深度上限）。
- [ ] `agent_run_workspace_list_entry` 同步填新字段。
- 验证：`cargo build -p agentdash-api`；手测 `GET /projects/:id/agent-runs` 返回收束结果。

## Step 4 — 契约补 lineage（workspace view）
- [ ] 新增 `AgentRunLineageRef` DTO；`AgentRunWorkspaceView` 增 `parent?` / `children`。
- [ ] `agent_run_workspace_view` / 组装处用 `find_parent` / `list_children` 填充 + title。
- 验证：`cargo build -p agentdash-contracts -p agentdash-api`。

## Step 5 — 重生成 TS 契约
- [ ] `cargo run -p agentdash-contracts --bin generate_contracts_ts`。
- 验证：git diff 仅 generated 文件；前端 `tsc` 不报缺字段。
- 回滚点：generated 文件可重生成。

## Step 6 — 前端侧栏自适应 + 标识
- [ ] [AgentRunShortcutList.tsx](../../../packages/app-web/src/components/layout/AgentRunShortcutList.tsx)：ResizeObserver 自适应可见条数 + "+N 更多" + subagent 角标，去滚动条。
- 验证：`pnpm --filter app-web typecheck && lint`；手测拖拽窗口高度。

## Step 7 — 前端主区列表递归折叠
- [ ] [active-agent-run-list.tsx](../../../packages/app-web/src/features/agent/active-agent-run-list.tsx)：递归可展开行（子行复用同组件，任意深度），一跳一拉懒加载 children，depth 缩进 + visited/深度上限防御；过滤作用于 root。
- 验证：typecheck/lint；手测多层展开/折叠 + 搜索 + 构造一个含孙节点的 Run。

## Step 8 — 前端右侧 identity bar + 修 role 倒灌
- [ ] [AgentRunWorkspacePage.tsx](../../../packages/app-web/src/pages/AgentRunWorkspacePage.tsx)：identity bar（kind + subject + parent/children 跳转）。
- [ ] [LifecyclePages.tsx:142,241](../../../packages/app-web/src/pages/LifecyclePages.tsx)：显示 `agent_kind` 为主，role 作副标识。
- 验证：typecheck/lint；手测 subagent 会话与主 Run 会话的 identity bar。

## Step 9 — 收尾验证（对齐 AC）
- [ ] 走查 AC1–AC6。
- [ ] `cargo build` 全量 + 前端 typecheck/lint。
- [ ] trellis-check：spec 合规 + 跨层数据流。

## Validation Commands
```
cargo build -p agentdash-domain
cargo build -p agentdash-application
cargo build -p agentdash-contracts -p agentdash-api
cargo run -p agentdash-contracts --bin generate_contracts_ts
pnpm --filter @agentdash/app-web typecheck
pnpm --filter @agentdash/app-web lint
```
（具体 pnpm filter 名以 package.json 为准，Step 6 前先核对。）

## Review Gates
- 后端三步（Step 1-4）完成后做一次编译+手测 gate，再进契约生成。
- 前端三步完成后做一次整体 gate（AC1/2/3/5）。
