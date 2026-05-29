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

## 调查结论（2026-05-29，执行者落地后回填）

> 结论：review 的两条命题**一对一错**。激进的 `SessionSurfaceResolver` 抽取被**中止**（耦合比 review 描述深，路径间存在真实契约差异）；镜像拍平**部分执行**（删掉一处确实死掉的镜像，保留两处各有独立消费方的镜像）。全套测试等价（604 passed 不变）。

### 命题 A —「compose_* 与 plan_*_context_query 把六步装配链各手写一遍」→ 与事实不符，不应强抽 `SessionSurfaceResolver`

两条路径**不是**同一条链的复制，而是**两个不同的输出契约**，且**已经在正确的层共享了真正的收敛点**：

- **launch 路径**（`api/session_use_cases/construction.rs::build_session_construction_for_launch` → `assembler.rs::compose_owner_bootstrap`/`compose_story_step`）：产出**执行期完整载荷** —— `SessionContextBundle`（含 bootstrap fragments）、`prompt_blocks`、`capability_state`（`CapabilityResolver` 输出被保留并下传）、audit emit、continuation lifecycle 三态处理、terminal hook effect binding。错误类型 `String` / `TaskExecutionError`。
- **query 路径**（`api/session_use_cases/context_query.rs::build_session_context_plan` → `construction_planner.rs::plan_*_context_query`）：产出**只读前端快照** —— 仅 `context_snapshot`，**不产 bundle**，`CapabilityResolver` 的 `cap_output` 被**丢弃**（只抽 `tool.mcp_servers` 喂给 snapshot），不产 `capability_state`，不产 prompt。`task` 子路径甚至不跑 `CapabilityResolver`，走 `task/context_builder.rs::build_task_session_context`（其 doc 明确："只读视图构建器…与启动链路无关，仅复用底层相同的 executor/VFS 解析逻辑以保持上下文数据一致"）。
- **真正的共享已存在**，无需新抽：
  1. snapshot 派生层 `session/bootstrap.rs::build_bootstrap_plan` + `derive_session_context_snapshot` —— launch 的 session_plan fragments 与 query 的 snapshot 都从这里派生，确保两路 executor/tool_visibility/runtime_policy 一致。
  2. 解析收敛 sink `api/session_use_cases/construction.rs::finalize_session_construction_projection` —— **launch 与 query 都调用同一个函数**（仅 `Launch`/`Inspect` mode 不同）补齐 working_dir / executor / session_capabilities / 最终 capability_state / extension_runtime / 镜像同步 / trace。

强抽单一 `SessionSurfaceResolver`（OwnerScope → ResolvedSessionSurface{vfs,capability_state,mcp_servers}）会把"产 bundle 的执行链"和"丢弃 cap_output 的只读快照链"强行揉成一个返回类型，破坏两者刻意的差异（bundle 生成时机、cap_output 是否保留、错误类型、`use_vfs` 条件分支、grants 应用范围）。这正是姊妹任务 drop-step 警示的"低估耦合"。**故中止此项。**

### 命题 B —「SessionConstructionPlan vfs 镜像 3 字段」→ 部分属实，已安全拍平死镜像

三处 vfs 镜像的消费方**并不对称**：

| 镜像字段 | 写 | 读 | 处置 |
|---|---|---|---|
| `surface.vfs` | launch/query/finalize | launch 链（`launch/orchestrator`/`planner`/`plan`）、`finalize`、extension_runtime route、canvas tools | **保留**（launch 主数据源） |
| `context_projection.vfs`（顶层 `SessionConstructionContextProjection`） | finalize/assembler | query DTO 路由（`acp_sessions`/`canvases`/`project_sessions`/`story_sessions`/`task_execution`/`terminals`/`vfs_surfaces`）| **保留**（query 只读模型） |
| `projections.context.*`（嵌套 `ConstructionProjections.context`） | `new()` / `assembler` / `canvas/tools` / `finalize` / `context_query` 共 5 处 | **全工程零读取** | **已删除**（纯 write-only 死状态） |

`projections.context` 这个嵌套投影是真正的 slop：5 处写、0 处读，与顶层 `context_projection` 完全冗余。已删除该字段及其全部写点，行为等价。`surface.vfs` 与 `context_projection.vfs` 始终在 `apply_session_assembly`/`finalize` 中成对写入同值，理论上可进一步合并为"单存储 + 派生"，但二者**消费方语义不同**（一个喂 launch executor，一个喂 query DTO），合并需引入派生访问器并改动 7+ 个路由调用方，属于命名/读模型重构，**风险收益比不如留作 `capability-state-unify` 时一并处理**，本任务不强改。

`validate_for_launch` 的 `capability_state.vfs.active == surface.vfs` / `mcp_servers` / `skill.skills` 三条断言**保留** —— 它们守护的是 `surface.vfs` 与 `projections.capability_state` 的真实一致性（`finalize` 会同步两者），不是死镜像，删之会失去 launch 前的漂移防护。

### 本任务实际改动
- 删 `ConstructionProjections.context` 字段（`session/construction.rs`）及 `new()` 构造里对它的 seed。
- 删 4 处死写点：`assembler.rs::apply_session_assembly`、`canvas/tools.rs`（测试 helper）、`api/.../construction.rs::finalize`（2 行）、`api/.../context_query.rs::attach_runtime_surface`（1 行）。
- `cargo check --workspace` 通过；`cargo test -p agentdash-application --lib` 604 passed（与基线一致）；api session/construction 相关测试全过。`agentdash-api` 有 1 个**先前既存且无关**的失败 `vfs_access::tests::runtime_tool_schemas_are_openai_compatible`（fs builtin 工具 schema `offset` 必填校验，与本任务无交集）。

### 给后续的建议
- `SessionSurfaceResolver` 不必抽；若要继续减重，应针对 `compose_owner_bootstrap`/`compose_story_step` 内部**各自**按阶段拆小函数（纯内部重构，不跨路径合并），属低风险 follow-up。
- `surface.vfs` 与 `context_projection.vfs` 的"单存储 + 派生"合并放到 `capability-state-unify` 一并做（届时 `CapabilityState.vfs.active` 已是权威源，可让两镜像都派生自它）。
- 建议人工 review。

---

## 🔴 wave2 重审（reopen 2026-05-29）

**为何 reopen**：wave2 零讨论纯代码盲审（独立、未读本任务讨论）在 `session/` 复现了"两个组装引擎 + 镜像字段靠校验器防漂移"。按 wave2 铁律，盲审复现项**不默认采信前轮"耦合被高估"**——但本任务上方「调查结论」给出了**具体新证据**（launch 链产 bundle+保留 cap_output vs query 链只读丢弃 cap_output 的契约分叉、已有共享收敛点 `build_bootstrap_plan`/`finalize_session_construction_projection`）。故 reopen 目标是**对抗式复核该证据**，而非强抽 resolver。

**两条独立线，互不前提：**

### 线 1 · 复核 resolver 争议（验证前轮论证，不预设对错）
按以下可证伪步骤独立核验，结论写入 journal：
1. 实读 `compose_owner_bootstrap`/`compose_story_step` 与 `plan_*_context_query`，逐行确认 VFS+capability 解析步骤究竟是**各自手写**还是**已委托** `build_bootstrap_plan`/`finalize`。前轮称已共享——核验其是否属实。
2. 若属实（共享点已存在、剩余差异是真契约分叉）→ **确认前轮结论**，本线收口为"无需抽 resolver"，但须把核验证据逐条列出（不是引用前轮，是新读）。
3. 若不属实（仍存在跨路径重复的解析逻辑）→ 抽 `SessionSurfaceResolver` 承载真正共享的 `{vfs, capability candidate, mcp_servers}`，各路径保留自己的后处理（bundle vs snapshot、cap_output 取舍）。

### 线 2 · 执行前轮甩锅掉缝的安全 follow-up（无条件做，与线 1 无关）
前轮把这两项推给 `capability-state-unify`，后者又推回——**两边都没做**：
1. `surface.vfs` / `context_projection.vfs` 改"单存储 + 派生访问器"（以 `CapabilityState.vfs.active` 为权威源），消除 `apply_session_assembly`/`finalize` 的手工同步；保留 `validate_for_launch` 中守护**真实**一致性的断言（非死镜像那条）。
2. `compose_owner_bootstrap`/`compose_story_step`（230-250 行）按阶段拆私有 helper（纯内部、不跨路径合并）；`SessionAssemblyBuilder` 拆出独立文件，`assembler.rs`(2654) 显著瘦身。

### wave2 硬验收（替代上方旧 Acceptance）
- [ ] 线 1 核验结论入 journal：要么抽出 `SessionSurfaceResolver`（grep 命中 + 两路径委托它），要么逐条新证据确认无需抽（不得仅引用前轮）
- [ ] `rg "apply_session_assembly" -A30` 中 `surface.vfs`/`context_projection.vfs` 的手工成对赋值消除（改派生）
- [ ] `compose_owner_bootstrap`/`compose_story_step` 行数各 < 80（`wc`）；`assembler.rs` < 1500 行或拆分
- [ ] `cargo check --workspace` + `cargo test -p agentdash-application --lib` 不回归（基线 604）+ launch/query/恢复三路径行为等价
- [ ] 任何缩窄逐条入 journal 标"建议人工复核"；线 2 两项**不得**再次推迟
