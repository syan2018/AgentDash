# AgentRun 列表收束与会话身份建模

## Goal

让 AgentRun 列表面向用户收束信息：侧栏只列每个 Lifecycle 的"主 Run"，subagent 不再被拍平成顶层条目；主区列表支持折叠展开 subagent；右侧会话栏补回 Agent 身份、Subject 从属与 Lineage 父子关系。配套修正后端列表查询口径、契约缺字段，以及 `agent_role` 这个烂尾字段。

## Background / 现状问题

1. **侧栏列表**（[AgentRunShortcutList.tsx](../../../packages/app-web/src/components/layout/AgentRunShortcutList.tsx)）：全量渲染 + `overflow-y-auto` 滚动条；早期 session 列表的"按容器实测高度加载对应数量"自适应特性已丢失。
2. **列表口径错误**（[lifecycle_agents.rs:111-143](../../../crates/agentdash-api/src/routes/lifecycle_agents.rs)）：`get_project_agent_runs` 对每个 Run 的**每个 agent**都产出一条 entry，导致同一 Lifecycle 下派发的 subagent / companion 与主 Run 平级出现，信息发散。
3. **`agent_role` 是半死字段**：唯一写入点 [lifecycle_agent.rs:50](../../../crates/agentdash-domain/src/workflow/lifecycle_agent.rs) 恒为 `"primary"`（subagent 也走同一个 `new_root`），全仓无任何按值分支；区分主从的真值实际由 `AgentLineage` 承担。前端 [LifecyclePages.tsx:142,241](../../../packages/app-web/src/pages/LifecyclePages.tsx) 的 `agent_role || agent_kind` 因此恒显示 "primary"，反而挡住了有信息量的 `agent_kind`。
4. **右侧会话栏信息缺失**（[AgentRunWorkspacePage.tsx:616-663](../../../packages/app-web/src/pages/AgentRunWorkspacePage.tsx)）：header 只有徽章 + 标题 + runtime session id。`agent_kind` / `subject_associations` 数据已在 `AgentRunWorkspaceView` 契约里却未渲染；Lineage 父子关系契约里压根没有字段。

## Requirements

### R1 后端列表收束（主线）
- `GET /projects/:id/agent-runs` 改为**每个 Lifecycle 只产出主 Run**（lineage 控制树中无父节点的 agent）。
- 列表 entry 增加 subagent 摘要：至少 `subagent_count`；为右侧/主区折叠预留 children 引用。
- 判定主/从一律以 `AgentLineage` 为真值源，不得依赖 `agent_role` 的旧实现。

### R2 复活 `agent_role`
- 建 agent 时按角色写入真实值：主 Run = `primary`；派发的子 agent 按 lineage `relation_kind` 写 `subagent` / `companion` 等。
- `agent_role` 作为 lineage 的冗余快捷标记，必须与 lineage 写入保持一致（同一创建路径）。
- 修正前端 `agent_role || agent_kind` 倒灌：展示以 `agent_kind` 为主，`agent_role` 仅作主/从语义标识。

### R3 契约补 Lineage
- `AgentRunWorkspaceView` 增加 lineage 投影：本 Run 若为 subagent，给出 `parent`（父 Run/agent ref + 可读 title，供跳转）；给出 `children`（本 Run 派发的 subagent 列表）。
- 重新生成 TS 契约（`generate_contracts_ts`）。

### R4 侧栏列表（前端）
- 只渲染主 Run；存在 subagent 时显示简单标识（如 "N sub" 角标）。
- 高度恢复**自适应**：按容器实测高度计算可见条数，超出部分走"更多 / 查看全部"入口，去掉常驻滚动条。

### R5 主区列表（前端）
- [ActiveAgentRunList](../../../packages/app-web/src/features/agent/active-agent-run-list.tsx) 以主 Run 为行，支持**递归折叠展开**该 Run 下的 subagent（树形，任意深度，子行可继续下钻）。
- 现有搜索 / 状态过滤继续生效（过滤口径作用于主 Run，展开项随父显示）。
- 所有树遍历带 visited 防环 + 深度上限兜底（lineage 支持递归且无环检测）。

### R6 右侧会话栏身份建模
- header 区补 identity bar，展示：
  - Agent 身份：`agent_kind`（+ 必要时 role 标识）。
  - Subject 从属：服务的 task / story 及 subject label。
  - Lineage 父子：subagent 显示"← 隶属于 {父 Run}"可跳转；主 Run 显示"派发了 N 个 subagent"可展开/跳转。

## Constraints

- 现有单测多为早期顺手写、价值低，不为兼容旧测试束缚改动；行为可改（见用户 memory）。
- 前端跨后端 DTO 改动遵循 [Type Safety](../../spec/frontend/type-safety.md)，不引入字段别名兼容层。
- 后端遵循 [Repository Pattern](../../spec/backend/repository-pattern.md) 与分层边界；列表收束逻辑放在应用层查询服务，路由保持薄。
- `agent_role` 取值集合需在 domain 层显式约定（常量），不散落字符串字面量。

## Acceptance Criteria

- [x] AC1：一个含 1 主 Run + ≥1 subagent 的 Lifecycle，侧栏只出 1 条主 Run，并显示 subagent 数标识。（后端按 lineage root 收束 + 侧栏 `N sub` 角标）
- [x] AC2：主区列表能折叠/展开看到该主 Run 下的 subagent；含孙节点（递归）的 Run 可逐层钻入；搜索与状态过滤仍可用。（`AgentRunTreeRow` 递归懒加载）
- [x] AC2b：subagent_count 为整棵子树后代数（含孙及更深），非仅直接子节点。（`count_descendants` 传递闭包，单测覆盖）
- [x] AC3：右侧打开一个 subagent 会话时，identity bar 显示其 agent_kind、所属 subject、以及可跳转的父 Run；打开主 Run 时显示其派发的 subagent 数/入口。
- [x] AC4：新建的 subagent 在 DB 中 `agent_role` 不再恒为 `primary`，与 lineage relation 一致；前端不再出现"恒显示 primary"。（`agent_role_for_plan` + LifecyclePages 修复）
- [x] AC5：侧栏列表无常驻滚动条，窗口高度变化时可见条数随之自适应，超量有"更多"入口。（ResizeObserver + overflow-hidden）
- [x] AC6：`cargo` 编译 + 契约生成通过；前端 typecheck/lint 通过。（均验证；新增 + 既有单测绿）

## Notes

- 关键真值源：`AgentLineage`（[agent_lineage.rs](../../../crates/agentdash-domain/src/workflow/agent_lineage.rs)）+ 仓储 `find_parent` / `list_children`（[repository.rs:125-129](../../../crates/agentdash-domain/src/workflow/repository.rs)）。
- 列表收束性能：每个 Run 已 `list_by_run`，可在内存按 lineage 过滤 root，避免 N 次 `find_parent` 往返。
