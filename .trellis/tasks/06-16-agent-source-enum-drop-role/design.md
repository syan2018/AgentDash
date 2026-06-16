# Design — Agent 来源枚举收束 + 删除 agent_role

## 1. AgentSource 枚举

- 位置：`agentdash-domain` workflow 模块（与 `LifecycleAgent` 同域）。
- 形态：`enum AgentSource { ProjectAgent, TaskAgent, Routine, WorkflowAgent, WorkflowActivity, /* 视 dispatch source 补全 */ }`，`#[serde(rename_all = "snake_case")]`，实现 `FromStr` / `as_str()` 与现有 slug 双向映射（迁移与 DB 解析复用）。
- 真值源：以 [dispatch_service.rs](crates/agentdash-application/src/workflow/dispatch_service.rs) `agent_kind_from_source(&plan.source)` 现有派生关系反推完整变体集合，确保枚举 = 现有来源全集（避免遗漏 workspace_module/present 等边角 new_root）。

## 2. LifecycleAgent 字段变更

- `agent_kind: String` → `source: AgentSource`（或保留 `agent_kind` 名承载枚举，见 PRD Q1）。
- 删除 `agent_role: String`。
- `new_root(run_id, project_id, agent_kind: impl Into<String>)` → `new_root(run_id, project_id, source: AgentSource)`；`with_role(..)` 移除。

## 3. role 语义替代（删除后如何取 role）

现有依赖 role 的判定盘点后逐一替换：
- **primary/主 Run 判定** → 已有真值源：lineage 控制树 root（`build_lineage_forest` 的 `child_ids` 之外即 root）。列表 / workspace 收束已部分走 lineage，统一收口。
- **companion/subagent 判定** → lineage 边 `relation_kind` + 是否为某 agent 的 child。
- **subject 维度的角色** → `lifecycle_subject_association`（用户已指出 role 应由 subject 推导）。
- 提供 helper（如 `fn is_primary(agent, lineage_forest) -> bool`）替代散落的 `agent.agent_role == "primary"` 比较。

## 4. 持久化迁移

- 新 migration（编号续 0090+）：
  - 存量 `agent_kind` 自由字符串 → 枚举 snake_case 规范化（绝大多数已是合法 slug，仅校验/兜底未知值为某默认变体并 warn）。
  - `agent_role` 列 DROP。
- 遵循项目 migration history guard（pre-commit 已校验）。仓库为 append-only migration，不可改历史文件。
- repo 层 `lifecycle_agent_repo` 的 row ↔ entity 映射同步（读写 source、移除 role）。

## 5. 契约与前端

- 契约：`AgentRunView` / `AgentRunLineageRef` / 其它暴露处移除 `agent_role`、`agent_kind` 改 `source`（或保留名）。ts-rs 重新生成。
- 前端引用点（[AgentRunWorkspacePage.tsx](packages/app-web/src/pages/AgentRunWorkspacePage.tsx)、[LifecyclePages.tsx](packages/app-web/src/pages/LifecyclePages.tsx)）改用 source / lineage 推导；本次列表已不展示 role/kind，仅需删类型引用。

## 6. 拆分建议（parent + children）

规模大、跨层，建议 parent 任务下拆三个可独立验证的 child：
- **C1 domain+源收束**：定义 `AgentSource`、改 `new_root` 全部调用点、kind 字段语义化（不动 role）。
- **C2 删 role**：移除字段 + 40 处引用 + role 推导 helper + repo 映射。
- **C3 迁移+契约+前端**：DB migration、契约重生成、前端清理、端到端回归。
顺序：C1 → C2 → C3（C2 依赖 C1 的 lineage helper 收口；C3 依赖 C1/C2 的最终字段形态）。

## 7. 风险

- role 删除可能命中隐藏依赖（某些 bootstrap / companion gate 逻辑读 role）——需全量 grep `agent_role` 后逐一替换，不可仅删字段。
- 迁移不可逆（DROP 列）；需确认无外部消费方依赖 `agent_role`。
- 枚举变体若漏覆盖某 new_root 来源，解析会落默认值——C1 必须穷举全部 20 处 + dispatch source。
