# session 装配流水线收敛

> 病灶 3。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> **高风险深逻辑重构**，依赖：`dedup-naming-boilerplate` 之后（命名稳定）。完成后标注"建议人工 review"。

## Scope
`crates/agentdash-application/src/session/`。消除 assembler 与 construction_planner 的平行装配流水线，拍平 SessionConstructionPlan 字段镜像。

## 证据
- `assembler.rs:849` `compose_owner_bootstrap`/`:1104` `compose_story_step` 与 `construction_planner.rs:155/303` `plan_*_context_query` 把六步装配链（build_vfs→ensure_lifecycle_mount→append_canvas→apply_grants→CapabilityResolver→MCP merge）各手写一遍。
- `SessionConstructionPlan`(`construction.rs:35/63/114`) vfs 镜像 3 字段，`apply_session_assembly:125-134` 手工同步，`validate_for_launch:213-279` 5 条断言防漂移。
- `assembler.rs` 2654 行 god module；`compose_*` 230-250 行巨型函数。

## Approach
1. 抽 `SessionSurfaceResolver`：`OwnerScope + executor config → ResolvedSessionSurface{vfs, capability_state, mcp_servers}`。assembler 与 construction_planner 都委托它。
2. 拍平 `SessionConstructionPlan`：vfs/capability_state 单一存储（建议归 `CapabilityState`，其已含 `vfs.active`/`tool.mcp_servers`），其余按需派生；删镜像同步与一致性断言。
3. `compose_owner_bootstrap`/`compose_story_step` 按阶段拆函数，收敛到 ~50 行编排；`SessionAssemblyBuilder` 拆独立文件。

## Acceptance
- [ ] 六步装配链单一实现（`SessionSurfaceResolver`）
- [ ] `SessionConstructionPlan` 无 vfs 三处镜像；`validate_for_launch` 镜像一致性断言删除
- [ ] `compose_*` 函数显著瘦身
- [ ] `cargo check --workspace` 通过；session 相关测试通过

## Constraints
- 仅改 `crates/agentdash-application/src/session/`（及必要的调用方）。**不要 git commit**，orchestrator gate 后提交。
- **高风险**：行为须等价（启动/恢复/查询三路径产出一致）。完成后 commit message + journal 标注建议人工 review。
- 与 `capability-state-unify` 同改 session/，本任务先做。
