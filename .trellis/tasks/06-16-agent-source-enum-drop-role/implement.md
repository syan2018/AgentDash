# Implement — Agent 来源枚举收束 + 删除 agent_role

> 单会话实现，按 C1→C2→C3 顺序推进（不拆 child task）。决策：Q1=改名 `source`，Q2=列表暴露来源标签。

## C1. domain + 来源收束
- [ ] 新建 [agent_source.rs](crates/agentdash-domain/src/workflow/agent_source.rs)：`enum AgentSource`（9 变体，见 design §1）+ `as_str()` + `FromStr`（未知落 `Unknown`）+ `Default = Unknown`；mod.rs 导出。
- [ ] `LifecycleAgent.agent_kind: String` → `source: AgentSource`；`new_root(.., source: AgentSource)`。
- [ ] 改 17 处生产 `new_root` 调用点 + dispatch `agent_kind_from_source` 收口为返回 `AgentSource`（`ParentAgent → Subagent` 等，见 design §1）；测试 4 处用 `AgentSource::Unknown` / `ProjectAgent`。
  - tools.rs ×2 → `WorkspaceModule`；view_projector + subject_execution_control ×3 → `TaskAgent`；reuse_resolver + dispatch ×2 → `Routine`；session_association + agent_node_launcher → `WorkflowAgent`；classify → `ProjectAgent` / `WorkflowActivity`；command_policy + lifecycle_agents 测试 `PI_AGENT` → `ProjectAgent`；gate_control + hub tests `test` → `Unknown`。
- [ ] `cargo check -p agentdash-domain -p agentdash-application` 通过。

## C2. 删除 agent_role（纯删，无推导 helper）
- [ ] domain：删 `agent_role` 字段、`with_role`、`pub mod agent_role`、mod.rs re-export。
- [ ] dispatch_service：删 `agent_role_for_plan` + `.with_role(..)` + `use agent_role` import（`lineage_relation_kind` 若仅服务 role 也评估）。
- [ ] 删散落 pass-through：query.rs / types.rs / lifecycle_run_view_builder.rs / lifecycle_agents.rs（视图赋值 + 测试断言）/ lifecycle_contracts.rs。
- [ ] permission entity.rs:220 / policy.rs:56 的字符串字面量**不动**（非本字段）。
- [ ] `cargo check --workspace` 通过。

## C3. 迁移 + repo + 契约 + 前端
- [ ] migration `0014_agent_source_enum_drop_role.sql`：RENAME `agent_kind`→`source` + 存量值规范化 UPDATE + DROP `agent_role`（见 design §4）。`node scripts/check-migration-history.js` 通过。
- [ ] repo：lifecycle_anchor_repository（row/INSERT/SELECT 改 source、删 role）；mailbox + command_receipt repo INSERT 列清单 `agent_kind,agent_role`→`source`。
- [ ] 契约 workflow.rs：`AgentRunView`/`AgentRunLineageRef`/`AgentRunWorkspaceListEntry`（+ list child）`agent_kind`→`source`、删 `agent_role`；`pnpm run contracts:generate`。
- [ ] 前端：列表来源标签 `agentSourceLabel()` + 主/子行展示；AgentRunWorkspacePage / LifecyclePages 改 source、删 role badge。
- [ ] 端到端回归：列表收束 / 嵌套展开 / 来源标签 / workspace 父子导航。

## 验证命令
```bash
cargo check && cargo clippy --workspace && cargo test
node scripts/check-migration-history.js
pnpm run contracts:check
pnpm --dir packages/app-web check
```

## 风险 / 回滚
- DROP COLUMN 不可逆：role 已确认无消费方，存量基本恒 primary，安全。
- 枚举漏覆盖来源 → `FromStr` 落 `Unknown`（不 panic）；C1 调用点已穷举。
- RENAME COLUMN 牵动多处 SQL 字符串，逐一核对 mailbox/receipt repo。
