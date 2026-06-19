# 技术设计：AgentRun 列表收束与会话身份建模

## Design Goal

把"列表收束 / 身份建模"统一锚到一个真值源——`AgentLineage` 控制树。后端只暴露收束后的视图与 lineage 投影，前端不再自己拼装主从关系。`agent_role` 从烂尾字段升级为 lineage 的冗余快捷标记，仅用于查询过滤与展示，绝不作为关系真值源。

## 真值源与现状锚点

| 关注点 | 真值源 | 现状 |
| --- | --- | --- |
| 主/从关系 | `AgentLineage(parent_agent_id → child_agent_id, relation_kind)` | 已落库，仅 companion/dispatch 路径写入 |
| Agent 身份 | `LifecycleAgent.agent_kind` | 已有，前端被 `agent_role||agent_kind` 挡住 |
| Subject 从属 | `LifecycleSubjectAssociation` | 已投影到 workspace view 的 `subject_associations` |
| 角色快捷标记 | `LifecycleAgent.agent_role` | 恒 `"primary"`，需复活 |

- lineage repo 句柄：`state.repos.agent_lineage_repo`（`find_parent` / `list_children`，[repository.rs:125-129](../../../crates/agentdash-domain/src/workflow/repository.rs)）。
- `relation_kind` 取值（[dispatch_service.rs:887](../../../crates/agentdash-application/src/workflow/dispatch_service.rs)）：`spawn` / `delegation` / `resume` / `reuse` / `companion`。

## 一、agent_role 复活（domain + 创建路径）

### 取值约定
domain 新增常量模块（与 `bootstrap_status` 同文件 [lifecycle_agent.rs](../../../crates/agentdash-domain/src/workflow/lifecycle_agent.rs)）：

```
pub mod agent_role {
    pub const PRIMARY: &str = "primary";    // 控制树 root（主 Run）
    pub const SUBAGENT: &str = "subagent";  // spawn / delegation 派发
    pub const COMPANION: &str = "companion";// companion 派发
}
```

### 写入点
- `LifecycleAgent::new_root` 保持写 `PRIMARY`。
- 新增 `LifecycleAgent::new_child(run_id, project_id, agent_kind, role)` 或 `with_role(role)` builder。
- dispatch 路径 [dispatch_service.rs:374-384](../../../crates/agentdash-application/src/workflow/dispatch_service.rs)：当 `plan.parent_agent_id.is_some()` 时，在创建 agent 处按 `relation_kind` 映射 role（`spawn`/`delegation`→`SUBAGENT`，`companion`→`COMPANION`），与 lineage 同一创建路径写入，保证一致。
- companion 创建路径 [gate_control.rs](../../../crates/agentdash-application/src/companion/gate_control.rs) 的 child agent 同步写 `COMPANION`。
- **不做历史数据迁移**：旧数据 role 恒 primary，但收束查询以 lineage 为准（见下），role 仅作展示与新数据的快捷过滤；因此存量数据不会误判。

> 决策：关系真值用 lineage，不用 role。role 只是冗余。这样即便 role 写错或存量未回填，列表收束仍正确。

## 二、后端列表收束

### 契约（[workflow.rs](../../../crates/agentdash-contracts/src/runtime/workflow.rs)）
`AgentRunWorkspaceListEntry` 增补：
```
subagent_count: usize,
agent_role: String,          // 透出，便于前端标识
```
（children 详细列表不进侧栏列表 entry，避免 N×M 膨胀；右侧 workspace view 提供完整 children。）

### 查询（[lifecycle_agents.rs:97-150](../../../crates/agentdash-api/src/routes/lifecycle_agents.rs)）
当前：`list_by_project → 每 run list_by_run → 每 agent 一条 entry`。

**注意：lineage 控制树支持任意深度递归**（subagent 可再派发 subagent，[companion/tools.rs:311](../../../crates/agentdash-application/src/companion/tools.rs) `parent_agent_id = anchor.agent_id`，无 max_depth/环检测）。收束必须在内存里建整棵 forest，而非按节点递归打 DB。

改为，每个 run 内：
1. `agents = list_by_run(run.id)`；
2. 建邻接：对每个 agent `list_children`（或 `find_parent`）一次，得到 parent↔children 邻接，**在内存构建 forest**；
3. **root = 从未作为 child 出现的 agent**（lineage 无父，主 Run，深度无关恒唯一）；只对 root 产出 entry；
4. `subagent_count` = 该 root 子树的**传递闭包后代数**（整棵子树，非一层），走内存 DFS——配 `visited` 防环 + 软深度上限（如 32）兜底。

#### Repo 取舍
`AgentLineageRepository` 当前只有 `find_parent` / `list_children`，按 agent 粒度。列表场景需要"按 run 取全部 lineage"再建树：
- **(推荐) 内存建树**：`list_by_run` 返回全部 agent，对每个 agent 调一次 `list_children` 拿全邻接，内存建 forest + DFS 计后代。run 内 agent 数通常很小，N 次调用可接受，零 schema 改动；递归深度由 DFS 处理，不产生 DB 往返放大。
- **(备选) 加 repo 方法** `list_by_run(run_id) -> Vec<AgentLineage>`：一次取全 run 的 lineage，省掉 N 次 `list_children`，深/宽树更优。需在 trait + postgres + 测试 fake 同步实现。

> 初版选内存建树；若实测 run 内 agent 多再加 lineage `list_by_run`。无论哪种，遍历都带 `visited` 防环 + 深度上限。

## 三、契约补 Lineage（workspace view）

`AgentRunWorkspaceView`（Rust [workflow.rs](../../../crates/agentdash-contracts/src/runtime/workflow.rs)，TS [workflow-contracts.ts:64](../../../packages/app-web/src/generated/workflow-contracts.ts)）增补：
```
parent?: AgentRunLineageRef,        // 本 Run 为 subagent 时，指向父
children: Vec<AgentRunLineageRef>,  // 本 Run 派发的 subagent
```
新增 DTO：
```
AgentRunLineageRef { run_id, agent_id, agent_kind, agent_role, relation_kind, display_title }
```
- 组装位置：`agent_run_workspace_view` / `load_agent_run_workspace_snapshot`（[lifecycle_agents.rs:626-700](../../../crates/agentdash-api/src/routes/lifecycle_agents.rs)）。
- parent：`agent_lineage_repo.find_parent(agent_id)` → 取父 agent + 其 shell title。
- children：`agent_lineage_repo.list_children(agent_id)` → 每个子 agent + title + relation_kind。
- title 复用 workspace shell 的 `display_title` 解析逻辑（避免再造标题）。
- 改完跑 `cargo run -p agentdash-contracts --bin generate_contracts_ts` 重生成 TS。

## 四、前端

### 4.1 侧栏 [AgentRunShortcutList.tsx](../../../packages/app-web/src/components/layout/AgentRunShortcutList.tsx)
- 数据：后端已收束，直接渲染（不再含 subagent）。
- 标识：`subagent_count > 0` 时行尾显示 "N sub" 角标。
- 自适应高度：用 `ResizeObserver` 测列表容器高度，按单行高（约 40px）算 `visibleCount = floor(height / rowH)`；只渲染前 `visibleCount` 条，剩余显示"+N 更多"按钮（跳主区完整列表）。去掉 `overflow-y-auto`。
  - 参考早期 session 列表自适应实现：检索 git 历史/归档任务 `02-26-frontend-agent-session-mvp` 等是否留有 hook 可复用。

### 4.2 主区列表 [active-agent-run-list.tsx](../../../packages/app-web/src/features/agent/active-agent-run-list.tsx)
- `AgentRunRow` 拆为**递归可展开行**：`subagent_count>0` 显示展开箭头；展开后渲染 children，**每个子行复用同一可展开组件**，可继续向下钻（支持任意深度）。
- 展开按需懒加载：展开某行时 `fetchAgentRunWorkspace(runId, agentId)` 读其 `children` 渲染下一层（一跳一拉，避免一次性拉全树）。
- 递归渲染带 `depth` 缩进 + `visited`/depth 上限防御（数据异常时不无限展开）。
- 过滤/搜索作用于 root；展开的子行随父显隐。

### 4.3 右侧 identity bar [AgentRunWorkspacePage.tsx:616-663](../../../packages/app-web/src/pages/AgentRunWorkspacePage.tsx)
- header 下方加一行 identity bar（或并入 header）：
  - `agent_kind`（来自 `runtimeControl.agent` / workspace view `agent`）。
  - subject：从 `subject_associations` 取首个的 label/subject_ref，渲染 "task: xxx" / "story: xxx"，点击跳 subject。
  - lineage：`parent` 存在 → "← 隶属于 {parent.display_title}" 按钮跳 `/agent-runs/{parent.run_id}/{parent.agent_id}`；否则若 `children.length>0` → "{n} 个 subagent" 可展开/跳转。

### 4.4 修 agent_role 倒灌 [LifecyclePages.tsx:142,241](../../../packages/app-web/src/pages/LifecyclePages.tsx)
- 显示主体改为 `agent_kind`；`agent_role` 仅在需要时作为副标识（如非 primary 时显示徽章）。

## 数据流（subagent 打开会话时）

```
路由 /agent-runs/:runId/:agentId
  → fetchAgentRunWorkspace → AgentRunWorkspaceView{ agent, subject_associations, parent, children }
  → identity bar 渲染 kind + subject + parent 跳转
侧栏 fetchProjectAgentRuns → 收束后的主 Run 列表(含 subagent_count)
```

## 兼容性 / 回滚

- 契约新增字段为 additive；TS `parent?` optional、`children` 默认空数组，旧前端不读则无感。
- 列表收束是行为变更（subagent 不再出现在顶层）——这是预期目标，非回归。
- 回滚点：列表查询改动、契约改动、agent_role 写入改动相互独立，可分别 revert。

## 递归处理约定（贯穿前后端）

lineage 控制树深度无限制且无环检测，所有遍历统一遵守：
- 后端后代计数 / 建树：内存 DFS + `visited` 集合防环 + 软深度上限（如 32），超限截断并 `warn` 日志（不静默）。
- 前端主区树：一跳一拉懒加载 + `depth` 缩进；同样带 visited / depth 上限防御。
- 右侧 identity bar 的 parent/children 是一跳相对导航，天然支持任意深度，无需特殊处理。
- 收束口径（root = 无父节点）与深度无关，恒正确。

## 不做 / 边界

- 不做存量 agent_role 数据迁移（收束以 lineage 为真值源，存量 role 恒 primary 不影响判定）。
- 不加 lineage 深度上限的业务约束（只在遍历侧做防御性兜底，不限制实际派发深度）。
- 不改 runtime/session trace 语义（lineage 已与 RuntimeSessionLineage 解耦，见 [agent_lineage.rs](../../../crates/agentdash-domain/src/workflow/agent_lineage.rs) 注释）。
