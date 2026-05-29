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

> **2026-05-29 修订**：执行调查发现 Step lifecycle **不是死代码**而是 load-bearing 活跃 runtime（task 启动/续跑、companion overlay、VFS 投影、session 构造均依赖，Activity 轨无对应入口）。`drop-step-lifecycle` 因此从"删死代码"重新定性为"feature 迁移"，**降级冻结**，待 P0a/b/c 设计 + 人工决策（详见该 task prd 冲突记录）。其余后端任务经核查并不真正依赖 Step 删除，已解除依赖。

| child | 病灶 | 优先级 | 风险 | 状态/依赖 |
|---|---|---|---|---|
| `05-29-drop-step-lifecycle` | 2 | P0 | 高（feature 迁移，非删除） | **冻结**：需 P0a/b/c 设计 + 人工决策，不自主执行 |
| `05-29-dedup-naming-boilerplate` | 4/5/6 低风险 | P0 | 低（机械改名/supertrait/去重） | 独立，立即可做 |
| `05-29-app-infra-leak-to-spi` | 1 | P1 | 中（port 抽取 + 下沉） | 独立（与 Step 无关） |
| `05-29-infra-persistence-dedup` | 5 | P1 | 中（共享映射层） | session_repository 去重独立可做；workflow_repository discriminator 部分被 drop-step 冻结阻塞，本轮跳过 |
| `05-29-session-assembly-converge` | 3 | P1 | 高（深逻辑重构） | dedup-naming 之后；执行前需先调查确认（参考 drop-step 教训：review 描述可能低估耦合） |
| `05-29-capability-state-unify` | 4 | P2 | 高（深逻辑重构） | session-assembly 之后（同改 session/） |
| `05-29-frontend-server-state-refactor` | 前端 | P2 | 中（独立 TS 包） | 无（全程可并行） |

## 自主执行策略（用户离开期间）

**分支**：`refactor/architecture-slop-cleanup`（不 push，留待用户回来 review）。
**编译基线**：2026-05-29 `cargo check --workspace` exit 0（干净）。

**执行模型**：依赖驱动分波 + 每任务 build-gate + 逐任务 commit。subagent 执行用 opus(4.8)，失败回退 sonnet。

**波次（2026-05-29 修订后）**：
- ~~Wave 1：drop-step-lifecycle~~ → **冻结**（feature 迁移，非删除；见上）
- Wave 1（实际）：`dedup-naming-boilerplate`（crates/，机械）‖ `frontend-server-state-refactor`（packages/，独立）
- Wave 2：`app-infra-leak-to-spi`（reqwest/rmcp/rhai/tokio::process 下沉，与 Step 无关）
- Wave 3：`infra-persistence-dedup`（仅 session_repository 去重部分；workflow discriminator 跳过）
- Wave 4：`session-assembly-converge` → `capability-state-unify`（执行前先调查确认耦合，高风险标注人工 review）

**教训（drop-step 触发）**：review 的"病灶"严重度可能低估真实耦合（把 live 代码当死代码）。深逻辑任务执行前，subagent 必须先调查"实际是否如 review 所述"，不符即停并记录，不强改。

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

## 执行结果（2026-05-29 自主执行完成）

落地详情见 `docs/reviews/2026-05-29-slop-cleanup-review/01-execution-outcome.md`。11 个 commit，均过 build-gate + 测试。最终测试态：spi/application/infrastructure/executor/前端全绿，api 88 绿 + 1 预存无关失败。

各 child 终态：
- `dedup-naming-boilerplate` ✅ 落地（部分项按"行为不变"收窄，偏差已记录）
- `app-infra-leak-to-spi` ✅ 5 port 全下沉
- `infra-persistence-dedup` ✅ session_repository 去重（workflow discriminator 部分按冻结跳过）
- `session-assembly-converge` 🔴 **wave2 reopen**（原 ⚠️ 只删死镜像未抽 resolver）— 见下
- `capability-state-unify` 🔴 **wave2 reopen**（原 ⚠️ 只上移 delta 未合并 trait）— 见下
- `frontend-server-state-refactor` 🔴 **wave2 reopen**（原标 A/B 完成，但盲审查当前代码：react-query 仅 1 处用、12 store 仍手搓 server-state，stage A 实为过度宣称）— 见下
- `drop-step-lifecycle` ❄️ 冻结（非死代码，需 P0a/b/c feature 迁移 + 人工决策）

**元结论（已被 wave2 部分纠偏）**：机械/结构类发现全部准确；但"3 个最深病灶均证明 review 高估耦合"过早下定论——见下 reopen。

## 2026-05-29 wave2 reopen（用户拍板）

第二波零讨论纯代码盲审独立复现了上述三个标"待收/⚠️/🟡"child 的残留。用户定性：**盲审又翻出来 = 没收干净，不采信"高估耦合"作默认开脱**。三项 reopen（详见各自 prd 的「🔴 wave2 重审」段，含硬验收 + 对抗式复核）：

- `session-assembly-converge`：线 1 对抗式复核 resolver 争议（验证而非预设前轮对错）；线 2 执行前轮与 capability **互相推卸、两边都没做**的 `surface.vfs/context_projection.vfs` 单存储派生 + compose_* 拆函数 + 2654 行 god module 瘦身。
- `capability-state-unify`：线 1 复核 trait 正交性；线 2 收掉前轮未碰的 `hooks::CapabilityDelta`≡`connector::SetDelta` 同 crate 同结构重复。
- `frontend-server-state-refactor`：stage A 真落地（store 迁 react-query）、stage C/D，及双源真理（project active id 两份）、跨 store `getState()` 命令式耦合、`workflowStore.selectedActivityKey` 派生字段等盲审新点。

> **纠偏**：session/capability 的前轮论证有具体证据、值得对抗式复核（不是预设其错）；frontend 则是实打实的过度宣称。"高估耦合"今后须经独立复核证成，不作免做理由。新债务并入 parent `05-29-slop-cleanup-wave2`。

## Notes
- parent 保持 planning，待全部 child archive 后再做集成 review 并归档（遵循"父任务不要早归档"）。
- 高风险 commit（699b11cc/4ff640fb）建议人工 review 后再决定 child 归档。
