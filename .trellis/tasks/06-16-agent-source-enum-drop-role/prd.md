# PRD — 收束 Agent 来源枚举 + 删除 agent_role

## 目标与用户价值

`LifecycleAgent.agent_kind` 与 `agent_role` 当前是创建期硬编码的自由字符串，语义混乱、对用户无意义，且已泄漏到列表 UI（`06-16-agentrun-list-inline-subagents` 已临时停止展示）。本任务把二者收束为一套**标准化的「Agent 创建/启动来源」枚举**，并删除冗余的 `agent_role`，role 语义改由 subject 关联 / lineage 控制树推导。

## 背景与确认事实（来自代码勘察）

- `LifecycleAgent`（[lifecycle_agent.rs](crates/agentdash-domain/src/workflow/lifecycle_agent.rs)）：`agent_kind: String`、`agent_role: String`。
- `agent_role` 域注释自承：「冗余快捷标记…存量数据可能恒为 primary…任何收束/嵌套判定都应回到 lineage，不得仅依赖此字段」。即 role 已非真值源。
- 主从真值源是 `AgentLineage` 控制树（root = 主 Run）；subject 归属来自 `lifecycle_subject_association`。
- `agent_kind` 硬编码于 **20 处 `new_root(...)`**，取值集合：`project_agent` / `task_agent` / `routine` / `workflow_agent` / `workflow_activity`（测试另有 `PI_AGENT` / `test`）。
- **已存在来源抓手**：[dispatch_service.rs](crates/agentdash-application/src/workflow/dispatch_service.rs) 的 `agent_kind_from_source(&plan.source)` —— kind 实际从某 `source` 派生，说明「来源」概念已部分存在，可作为枚举收束起点。
- 持久化：`agents` 表 `agent_kind` / `agent_role` 列定义于 [0001_init.sql](crates/agentdash-infrastructure/migrations/0001_init.sql)。
- Rust 侧 `agent_role` / `.agent_kind` 引用约 40 处；契约侧 `AgentRunView` / `AgentRunLineageRef` 等暴露这两字段；前端 [AgentRunWorkspacePage.tsx](packages/app-web/src/pages/AgentRunWorkspacePage.tsx)、[LifecyclePages.tsx](packages/app-web/src/pages/LifecyclePages.tsx) 有引用。

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

## 待澄清

- Q1：`agent_kind` 列直接复用（存枚举 snake_case）还是改名 `source`？（影响 migration 与契约命名）
- Q2：是否需在 UI 暴露 `AgentSource`（图标/标签区分 project/routine/workflow 来源）？
