# 架构 slop 清理：分层净化与去重（parent）

## Goal

把 2026-05-29 七路并行 review 的发现系统性落地：消除分层泄漏、删除双轨 lifecycle 死代码、消除跨后端逐行重复、拆解 god file。让项目朝"骨架不变、执行收紧"的方向收敛。

源审查：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`（事实源，含全部 file:line 证据）。

## 背景关键决策

- **Lifecycle 双轨：Activity 为目标轨，Step 轨整体删除**（用户 2026-05-29 拍板）。
- 预研期未上线：无向后兼容/字段兼容/回退包袱，可激进删除；但 DB schema 变更必须走 migration。

## Requirements（按病灶）

1. **病灶 1**：application 层不得直接持有基础设施（reqwest/rmcp/rhai/tokio::process）。抽 SPI port，实现下沉 infrastructure。
2. **病灶 2**：删除 Step lifecycle（`step_states`/`LifecycleStepState`/`activate_step` 等 + 旧 `LifecycleDefinition` 双轨），Activity lifecycle 成唯一模型。
3. **病灶 3**：session 装配流水线去重，单一 `SessionSurfaceResolver`，拍平 `SessionConstructionPlan` 字段镜像。
4. **病灶 4**：消除重复类型（`McpTransportConfig` 等）、命名漂移（`runtime*`/`RelayVfsService`）、概念散落（capability 四处、extension 四文件）。
5. **病灶 5**：消除逐行重复（infra sqlite/postgres、bridge spawn、MCP 连接、`SessionPersistence` 手抄）。
6. **病灶 6**：消除手写 JSON poking、stringly error、样板 `map_err`。
7. **前端**：引入 react-query、补齐 service 层、`@agentdash/ui` 基线、拆 god component。

## 任务图（child）

| child | 病灶 | 优先级 | 风险 | 依赖 |
|---|---|---|---|---|
| `05-29-drop-step-lifecycle` | 2 | P0 | 中（跨 3 crate 删除） | 无（最先做，缩小他人编辑面） |
| `05-29-dedup-naming-boilerplate` | 4/5/6 低风险 | P0 | 低（机械改名/supertrait/去重） | drop-step 之后 |
| `05-29-app-infra-leak-to-spi` | 1 | P1 | 中（port 抽取 + 下沉） | drop-step 之后（agent_executor 已瘦身） |
| `05-29-infra-persistence-dedup` | 5 | P1 | 中（共享映射层） | drop-step 之后（kind 列/step 删除已定） |
| `05-29-session-assembly-converge` | 3 | P1 | 高（深逻辑重构） | dedup-naming 之后 |
| `05-29-capability-state-unify` | 4 | P2 | 高（深逻辑重构） | session-assembly 之后（同改 session/） |
| `05-29-frontend-server-state-refactor` | 前端 | P2 | 中（独立 TS 包） | 无（全程可并行） |

## 自主执行策略（用户离开期间）

**分支**：`refactor/architecture-slop-cleanup`（不 push，留待用户回来 review）。
**编译基线**：2026-05-29 `cargo check --workspace` exit 0（干净）。

**执行模型**：依赖驱动分波 + 每任务 build-gate + 逐任务 commit。subagent 执行用 opus(4.8)，失败回退 sonnet。

**波次**：
- Wave 1：`drop-step-lifecycle`（后端基础）‖ `frontend-server-state-refactor`（独立，全程并行）
- Wave 2：`dedup-naming-boilerplate`（机械，后端代码已稳定）
- Wave 3：`app-infra-leak-to-spi` ‖ `infra-persistence-dedup`（不同 crate，worktree 隔离）
- Wave 4：`session-assembly-converge` → `capability-state-unify`（同改 session/，串行）

**Gate（每任务完成后由 orchestrator 执行，subagent 不得 commit）**：
1. `cargo check --workspace`（Rust）/ 前端 `pnpm -C packages/app-web exec tsc --noEmit` 必须通过。
2. 通过 → `git add -A && git commit`（`type(scope): 中文信息` + 分点备注）。
3. 失败 → 一次定向修复尝试；仍失败则回退该任务，journal 记录原因并跳过，不污染后续波次。

**高风险任务（session-assembly / capability-unify）**：通过 gate 后单独 commit，commit message + journal 标注"建议人工 review"，因为深逻辑重构超出纯机械验证。

## Acceptance Criteria（parent，待全部 child archive 后核对）

- [ ] 每个 commit 点 `cargo check --workspace` 通过
- [ ] application crate 不再直接构造 `reqwest::`/`rmcp::`/`rhai::`/`tokio::process`（病灶 1）
- [ ] domain/application/infrastructure 中 `step_states`/`LifecycleStepState` 归零（病灶 2）
- [ ] `McpTransportConfig` 全工作区单一定义（病灶 4）
- [ ] infra session_repository 重复率显著下降（病灶 5）
- [ ] 各 child 的 acceptance 全部满足
- [ ] 最终集成 review：跨 child 接口一致、无残留死代码

## Notes
- parent 保持 planning，待全部 child archive 后再做集成 review 并归档（遵循"父任务不要早归档"）。
