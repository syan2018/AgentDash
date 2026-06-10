# Implement — Capability Update Primitives（parent 编排）

> 本任务为 parent，实际编码在 4 个 child 中进行。本文给出跨 child 的顺序、验证门与集成验收。

## 执行顺序与依赖（2 child）

```
Child A (原语 + workspace_module 收口 + 前端, 修 bug)  ──►  Child B (skill/companion 旁路退役)
```

- Child A 先行且自包含：内部顺序 A原语(纯加性) → B收口(行为切换) → C前端；独立交付修 bug 价值。
- Child B 依赖 Child A 落地的 `AccumulationPolicy` 词汇；最敏感，单独隔离风险。

## 每个 child 的统一验证门（required）

```bash
cargo build --workspace
cargo test --workspace                      # 或受影响 crate 的定向测试
cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check   # pnpm contracts:check
pnpm --filter app-web typecheck             # 涉及前端时
node scripts/check-migration-history.js     # 涉及 DB 时
```

## 集成验收（parent，全部 child archive 后）

1. 6 维度归类表在代码 `policy()` 与文档中一致（验收 1/4）。
2. 端到端回归：workspace_module set→clear→全部可见；set→保持→受限；新 agent→全部可见（验收 3）。
3. grep 确认旁路代码已删除：
   - （Child A）frame_builder workspace_module 混合 match 臂、`with_visible_workspace_module_refs`；frame_construction/mod.rs L363-374 workspace_module 直接赋值；composer `unwrap_or_default`（workspace_module 路径）；
   - （Child B）frame_builder skill carry-forward（L58-61）。
4. tool/vfs/mcp 行为零回归（验收 7）：跑 capability_state 相关既有测试。
5. 全 workspace 构建 + 测试 + contracts:check + app-web typecheck 通过。

## 回滚点

- 每个 child 一个 commit，可独立 revert。
- workspace_module 收口（Child 2）若回归，可恢复 frame_construction 直接赋值 + 旧 frame_builder 臂（`visible_workspace_module_refs_json` 列未删，数据通路可恢复）。

## 完成后

- parent 留 planning，待 4 child 全部 archive 后做集成 review 再 archive（见 [[feedback_parent_task_no_early_archive]]）。
- 开 PR，关联取代 PR #45 Child 4 的说明。
