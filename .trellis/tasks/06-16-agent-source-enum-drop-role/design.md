# Design — Agent 来源枚举收束 + 删除 agent_role

## 1. AgentSource 枚举（诚实集合，已返工收窄）

- **位置**：定义在 [lifecycle_agent.rs](crates/agentdash-domain/src/workflow/lifecycle_agent.rs)，与 `LifecycleAgent` 同文件——它是 agent 的内在身份属性，不是 workflow 概念，不再放孤儿文件 `agent_source.rs`。
- 形态：`#[serde(rename_all = "snake_case")]`，`Copy`，`as_str()` + `FromStr`（未识别落 `Unknown`）+ `Default = Unknown`。

```rust
pub enum AgentSource {
    ProjectAgent,  // ExecutionSource::User|Api|ProjectAgent 出生
    Routine,       // routine 出生
    Subagent,      // ExecutionSource::ParentAgent spawn 出生
    WorkflowAgent, // orchestration workflow activity 节点 agent
    Unknown,       // 唯一兜底：未识别 / 历史 / 死路径
}
```

- **变体来源（实地穷举后收窄）**：生产中 `new_root` 只有 2 个真实出生点——`dispatch_service::create_agent`（出生唯一路径，非 resume/reuse）与 `agent_node_launcher`（orchestration activity）。其余 ~15 处全是 `#[cfg(test)]` 夹具。早期把测试 slug（`task_agent`/`workflow_activity`/`create-workspace-module`...）一并 union 进来 = 伪变体，已删除。
- **删除 `Migration`**：`ExecutionSource::Migration` 全仓库**无构造点**（死变体），故对应 AgentSource 同样删除；防御性 match 落 `Unknown`。
- **出生唯一映射**（`agent_source_from_execution_source`，仅 `create_agent` 调用一次，绝不在状态/交互更新回写）：`User|ProjectAgent|Api → ProjectAgent`，`Routine → Routine`，`ParentAgent → Subagent`，`Migration → Unknown`。
- **关键澄清**：`AgentSource`（出生一次的身份）≠ `ExecutionSource`（每次 dispatch/状态更新的触发来源）。早期 `agent_kind_from_source` 把身份耦合到 per-execution 触发轴，是这次要纠正的「变态耦合」。
- 验证：经全量 grep，无任何生产逻辑分支读 `agent_kind`/`source`（与 `agent_role` 一样纯展示）。

## 2. LifecycleAgent 字段变更

- `agent_kind: String` → `source: AgentSource`（Q1 已定：字段 + 列同步改名 `source`）。
- 删除 `agent_role: String` 及 `with_role(..)`、`pub mod agent_role { .. }`、mod.rs re-export。
- `new_root(run_id, project_id, agent_kind: impl Into<String>)` → `new_root(run_id, project_id, source: AgentSource)`。

## 3. role 删除（确认无需替代推导）

全量 grep 确认 `agent_role` **无分支逻辑消费**：
- 主从/嵌套判定早已走 lineage 控制树（`build_lineage_forest`），列表/workspace 收束不读 role。
- dispatch 侧 `agent_role_for_plan` 仅在 create 时**写入** role，无读取方 → 连同 `with_role` 一并删除。
- permission 侧 `"agent_role:patrol"`（entity.rs:220）、`"...agent_role ∩..."`（policy.rs:56）是字符串字面量/reason 文案，与字段无关，**不动**。
- 故**无需** `is_primary` 等 role 推导 helper；纯删字段 + 删散落 pass-through 引用。

## 4. 持久化迁移（0014）

- 新 migration `0014_agent_source_enum_drop_role.sql`：
  1. `ALTER TABLE lifecycle_agents RENAME COLUMN agent_kind TO source;`
  2. 存量值规范化 UPDATE（诚实集合）：`project_agent → project_agent`，`routine|routine_agent → routine`，`subagent|child_agent → subagent`，`workflow_agent → workflow_agent`，**其余全部**（`migration_agent`/`task_agent`/`workflow_activity`/`test`/`PI_AGENT`/NULL...）→ `unknown`。
  3. `ALTER TABLE lifecycle_agents DROP COLUMN agent_role;`
- 遵循 migration history guard（append-only，不改历史文件）。
- repo 层同步：`lifecycle_anchor_repository` row 结构/INSERT/SELECT 改 `source`、移除 `agent_role`；`agent_run_mailbox_repository` / `agent_run_command_receipt_repository` 的 INSERT 列清单 `agent_kind,agent_role` → `source`（去掉 agent_role）。

## 5. 契约与前端

- 契约：`AgentRunView` / `AgentRunLineageRef` / `AgentRunWorkspaceListEntry`（及 list child）移除 `agent_role`、`agent_kind` 改 `source: String`（DB 存 snake_case，契约层维持 string 即可）。ts-rs 重新生成。
- 前端：
  - **列表（Q2 暴露）**：[active-agent-run-list.tsx](packages/app-web/src/features/agent/active-agent-run-list.tsx) 主行/子行在身份标识旁增「来源标签」`<SourceTag>`，新增 [agent-source.ts](packages/app-web/src/lib/agent-source.ts) `agentSourceLabel(source)` 映射（Project / Routine / Subagent / Workflow；unknown→null 不渲染）。
  - [AgentRunWorkspacePage.tsx](packages/app-web/src/pages/AgentRunWorkspacePage.tsx)：`identityAgentRole` 删除，`identityAgentKind`/`lineageParent.agent_kind` → `source`。
  - [LifecyclePages.tsx](packages/app-web/src/pages/LifecyclePages.tsx)：role badge 删除，`agent_kind` → `source`。

## 6. 实施切分（单任务、顺序推进）

复盘后规模可控（C2 确认为纯删、无推导 helper），不拆 child task，单任务内按阶段推进：
- **C1 domain+源收束**：定义 `AgentSource`、改 `new_root` 全部调用点、字段改 `source`（不动 role）。
- **C2 删 role**：移除字段 + 散落 pass-through 引用（无 helper）。
- **C3 迁移+契约+前端**：DB migration（rename+normalize+drop）、repo SQL、契约重生成、前端 source 标签 + 删 role 展示、端到端回归。
顺序 C1 → C2 → C3（C3 依赖 C1/C2 的最终字段形态）。

## 7. 风险

- role 删除可能命中隐藏依赖（某些 bootstrap / companion gate 逻辑读 role）——需全量 grep `agent_role` 后逐一替换，不可仅删字段。
- 迁移不可逆（DROP 列）；需确认无外部消费方依赖 `agent_role`。
- 枚举变体若漏覆盖某 new_root 来源，解析会落默认值——C1 必须穷举全部 20 处 + dispatch source。
