# PRD — 收束 Agent 来源枚举 + 删除 agent_role

## 目标与用户价值

`LifecycleAgent.agent_kind` 与 `agent_role` 当前是创建期硬编码的自由字符串，语义混乱、对用户无意义，且已泄漏到列表 UI（`06-16-agentrun-list-inline-subagents` 已临时停止展示）。本任务把二者收束为一套**标准化的「Agent 创建/启动来源」枚举**，并删除冗余的 `agent_role`，role 语义改由 subject 关联 / lineage 控制树推导。

## 背景与确认事实（来自代码勘察）

- `LifecycleAgent`（[lifecycle_agent.rs](crates/agentdash-domain/src/workflow/lifecycle_agent.rs)）：`agent_kind: String`、`agent_role: String`。
- `agent_role` 域注释自承：「冗余快捷标记…存量数据可能恒为 primary…任何收束/嵌套判定都应回到 lineage，不得仅依赖此字段」。即 role 已非真值源。
- 主从真值源是 `AgentLineage` 控制树（root = 主 Run）；subject 归属来自 `lifecycle_subject_association`。
- `agent_kind` 散见于 ~17 处 `new_root(...)`，但**生产出生点只有 2 个**（`dispatch_service::create_agent` + `agent_node_launcher`），其余全是 `#[cfg(test)]` 夹具；早期把测试 slug 误当真实来源 union 进枚举（已返工删除）。历史上**存在两套互不一致的 slug**（已实地核对）：
  - dispatch 路径 `agent_kind_from_source(&plan.source)` 产出：`project_agent` / `routine_agent` / `child_agent` / `migration_agent`；
  - 散落字面量产出：`task_agent`（view_projector / subject_execution_control×3）、`routine`（reuse_resolver / dispatch×2）、`workflow_agent`（session_association / agent_node_launcher）、`workflow_activity`（classify）、`project_agent`（classify）、`create-workspace-module` / `present-workspace-module`（workspace_module/tools）；测试另有 `PI_AGENT` / `test`。
  - 两套并存正是要收束的根因 —— 枚举全集须覆盖上述全部语义。
- **`ExecutionSource`（`User/Routine/ParentAgent/ProjectAgent/Api/Migration`）是「触发来源」轴，与 `agent_kind`（Agent 种类/语义轴）不是一回事**，故 `AgentSource` 独立定义，不复用 `ExecutionSource`；二者由 `agent_kind_from_source` 做映射。
- **`agent_role` 经全量 grep 确认无任何分支逻辑消费**（lifecycle_agents.rs:157 注释自承「主从真值源是 lineage，不依赖 agent_role」；permission 侧 `agent_role:patrol` / reason 文案是字符串字面量，非本字段）。即删除 role **无需** role 推导 helper，是纯冗余字段删除。
- 持久化：`agents` 表 `agent_kind` / `agent_role` 列定义于 [0001_init.sql](crates/agentdash-infrastructure/migrations/0001_init.sql)；列名还散见于 mailbox / command_receipt repo 的 INSERT 列清单。**当前最新 migration 为 0013，新增编号 0014。**
- Rust 侧引用：`agent_role` 14 文件、`agent_kind` 10 文件；契约 `AgentRunView` / `AgentRunLineageRef` / `AgentRunWorkspaceListEntry` 暴露这两字段；前端 [AgentRunWorkspacePage.tsx](packages/app-web/src/pages/AgentRunWorkspacePage.tsx)、[LifecyclePages.tsx](packages/app-web/src/pages/LifecyclePages.tsx) 有 role/kind 展示。

## 需求

1. **定义 `AgentSource` 枚举**（暂名）：覆盖现有全部来源（project_agent / task_agent / routine / workflow_agent / workflow_activity / …），表达「这个 Agent 因何被创建/启动」。domain 类型，序列化 snake_case。
2. **收束创建入口**：将 20 处 `new_root(_, _, "<slug>")` 改为传 `AgentSource` 变体；以 `agent_kind_from_source` 现有派生逻辑为基础统一。
3. **`agent_kind` 字段语义化**：用 `AgentSource` 取代自由字符串（DB 存其 snake_case 表示 + 解析校验），占据原 role 生态位。
4. **删除 `agent_role`**：移除字段及 ~40 处引用；凡需 role 语义处改由 lineage（primary = 控制树 root）/ subject 关联推导。
5. **契约 + 前端对齐**：契约移除 `agent_role`、`agent_kind` 改 `AgentSource`；前端引用点同步。
6. **数据迁移**：存量 `agent_kind` slug → 枚举变体映射；`agent_role` 列删除（存量基本恒为 primary，安全）。

## 验收标准

- [ ] `AgentSource` 枚举定义且覆盖全部现有来源；无残留自由字符串 `new_root` slug。
- [ ] `agent_role` 字段及全部引用删除；编译通过，role 语义由 lineage/subject 推导处均有替代实现。
- [ ] DB migration 完成列变更并对存量数据做映射；migration guard 通过。
- [ ] 契约重新生成（ts-rs）；前端 typecheck/lint/test 通过；后端 check/clippy/test 通过。
- [ ] 现有依赖 role/kind 的行为（列表收束、嵌套判定）回归无破。

## Out of Scope

- 列表 UI 展示形态（已在 `06-16-agentrun-list-inline-subagents` 完成；本任务若需在 UI 暴露 `AgentSource` 作为收尾小项）。
- Story/Task subject 模型本身的重构（另见 codex-agent 的 `story-task-subject-model-cleanup`，需协调 role-from-subject 的推导依赖）。

## 已决策（原待澄清）

- **Q1 → 改名 `source`**：列与契约字段统一改名 `source`，彻底摆脱误导性的 `agent_kind`。migration 含 `RENAME COLUMN agent_kind TO source`，并同步 mailbox / command_receipt repo 的 INSERT 列清单。
- **Q2 → 在列表暴露 `AgentSource`**：AgentRun 列表行展示来源标签（snake_case → 人类可读映射，复用前端既有标签风格），主行与子行共用。
