# Implement — Agent 来源枚举收束 + 删除 agent_role

> 规划态。建议拆 C1→C2→C3 三 child（见 design §6）。以下为单线展开的有序清单，可按 child 切分。

## C1. domain + 来源收束
- [ ] 盘点全部来源：`grep -rn "new_root(" crates --include=*.rs`（20 处）+ `agent_kind_from_source` 的 source 全集。
- [ ] 在 domain 定义 `AgentSource` 枚举（snake_case 序列化 + `FromStr`/`as_str` 双向映射）。
- [ ] `LifecycleAgent.agent_kind` 改承载 `AgentSource`（PRD Q1 决定是否改名 `source`）。
- [ ] `new_root` 签名改收 `AgentSource`；逐一改 20 处调用点；`agent_kind_from_source` 收口为返回 `AgentSource`。
- [ ] `cargo check` 全 workspace 通过。

## C2. 删除 agent_role
- [ ] `grep -rn "agent_role" crates --include=*.rs`（~40 处）逐一分类：primary 判定 / companion 判定 / subject 角色 / 纯展示。
- [ ] 新增 role 推导 helper（基于 lineage forest / subject association），替换散落比较。
- [ ] 删除 `LifecycleAgent.agent_role` 字段 + `with_role`。
- [ ] repo row↔entity 映射移除 role。
- [ ] `cargo check` + 受影响单测修复。

## C3. 迁移 + 契约 + 前端
- [ ] 新 migration（0090+）：`agent_kind` 存量规范化为枚举 slug；`DROP COLUMN agent_role`。`node scripts/check-migration-history.js` 通过。
- [ ] 契约：移除 `agent_role`，`agent_kind`→`source`（或保留名）于 `AgentRunView`/`AgentRunLineageRef` 等；`pnpm run contracts:generate`。
- [ ] 前端 [AgentRunWorkspacePage.tsx](packages/app-web/src/pages/AgentRunWorkspacePage.tsx) / [LifecyclePages.tsx](packages/app-web/src/pages/LifecyclePages.tsx) 引用清理或改用 source。
- [ ] 端到端回归：列表收束 / 嵌套展开 / workspace 父子导航。

## 验证命令
```bash
cargo check && cargo clippy --workspace && cargo test
node scripts/check-migration-history.js
pnpm run contracts:check
pnpm --dir packages/app-web check
```

## 风险 / 回滚
- DROP COLUMN 不可逆：上线前确认无外部消费 `agent_role`。
- 枚举漏覆盖来源 → 解析落默认值；C1 穷举为硬性门槛。
- 与 codex-agent `story-task-subject-model-cleanup` 在「role 由 subject 推导」处可能耦合，需对齐。
