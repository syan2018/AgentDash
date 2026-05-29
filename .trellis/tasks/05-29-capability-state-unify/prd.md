# 能力状态机统一

> 病灶 4（capability 散落）。事实源：`docs/reviews/2026-05-29-slop-cleanup-review/00-synthesis.md`。
> **高风险深逻辑重构**，依赖：`session-assembly-converge` 之后（同改 session/，串行）。完成后标注"建议人工 review"。

## Scope
统一 capability 的"解析/演化/投影"三处散落，合并两套并行 dimension trait。

## 证据
- 散 4 处：`capability/resolver.rs`(1141, 静态归约)、`session/capability_state.rs`(1285, 运行时 transition/delta/replay)、`session/capability_projection.rs`(188, 派生)、`session/dimension/`。
- 两套 dimension trait 并行：`capability_state.rs:248` `CapabilityDimensionModule`（validate+replay）vs `dimension/mod.rs:19` `DimensionDelta`（delta+render），覆盖几乎相同维度（vfs/mcp/tool/skill）。
- 纯数据 delta 类型（`CapabilityStateDelta`/`VfsSurfaceDelta`）在 application，应在 spi/domain。

## Approach
1. 合并两套 dimension trait 为单一 `CapabilityDimension`（validate/replay/delta/render 同 trait）。
2. transition 应用收敛为单一 `CapabilityTransitionService`（live/pending/next-turn 统一入口，原 `hub/runtime_context_transition.rs` 并入）。
3. delta 纯数据类型上移 spi 或 domain。
4. capability_state.rs 瘦身为"调 transition + 存 persistence"的胶水。

## Acceptance
- [ ] 单一 dimension trait
- [ ] 单一 `CapabilityTransitionService` 作为能力切换唯一入口
- [ ] delta 类型不在 application
- [ ] `cargo check --workspace` 通过；capability/session 测试通过

## Constraints
- 仅改 `crates/`（application/spi/domain）。**不要 git commit**，orchestrator gate 后提交。
- **高风险**：能力切换行为须等价。完成后 commit message + journal 标注建议人工 review。
- 在 `session-assembly-converge` 之后做。

## 调查结论（2026-05-29，执行者落地后回填）

> 结论：与 drop-step / session-assembly 一致，review **高估了重复**。四处是清晰的阶段分工，两套 trait 是正交职责。**未合并 trait、未建单一 `CapabilityTransitionService`**。只执行了唯一安全且层次正确的子集：**delta 纯数据类型 + 计算函数上移 spi**。行为等价（application lib 604 passed 不变）。

### 命题 A —「四处散落是同一概念的冗余切分」→ 与事实不符，是 resolve→transition→derive→render 四阶段分工

| 文件 | 阶段 | 职责 | 输入/输出 |
|---|---|---|---|
| `capability/resolver.rs` | **解析期**（静态归约） | 纯函数把 agent/workflow contributions + MCP 候选归约为**初始** `CapabilityState` | `CapabilityResolverInput → CapabilityState`，无 before/after、无 replay |
| `session/capability_state.rs` | **运行期**（transition 应用） | 把 effect record 应用到 `CapabilityState`（`replay_*`）、构造 transition 值对象供事件 payload | live/pending/replay 状态算术 |
| `session/capability_projection.rs` | **投影期**（派生） | **异步 I/O**：从 VFS 发现 skills、从 mount 发现 guidelines、构建 baseline | 与状态算术无关，是 I/O bound 派生 |
| `session/dimension/` | **渲染期**（delta → UI/文本） | 把已算出的 `CapabilityStateDelta` 渲染成 `ContextFrameSection` + Markdown | `CapabilityStateDelta → section/text` |

强行揉成单一 service 会把"纯归约器 + 异步 I/O 派生器 + 状态变更引擎 + 文本渲染器"四种不同性质的东西捏在一起。**故不合并。**

### 命题 B —「两套 dimension trait 是真重复」→ 与事实不符，覆盖正交关注点

- `CapabilityDimensionModule`（`validate_declaration` + `validate_effect` + `replay_effect`）：**effect 应用**轴 —— 用 effect record **mutate** `CapabilityState`。维度集 = vfs/tool/mcp/**companion**。
- `DimensionDelta`（`has_changes` + `to_section` + `render_text`）：**渲染**轴 —— 把已算出的 `CapabilityStateDelta` 转成前端 section + Agent 文本。维度集 = **capability_key/tool_path**/mcp_server/vfs/**skill/tool_schema**。

二者**无任何同名方法、输入不同（effect record vs 计算后 delta）、输出不同（mutated state vs text/section）、维度集只部分重叠**（companion 只在前者；capability_key/tool_path/skill/tool_schema 只在后者）。合并会让每个 impl 背一堆 `unimplemented!`/no-op。**故不合并。**

### 实际改动（唯一安全子集：delta 类型上移）

`CapabilityStateDelta` / `VfsSurfaceDelta` / `NamedEntityDelta` / `SetDelta` / `DefaultMountDelta` 及计算函数 `compute_capability_state_delta`（连同其私有 helper `set_delta`/`named_entity_delta`/`vfs_surface_delta`）是纯数据 + 纯函数，只依赖 spi `CapabilityState` 与 domain `Vfs`/`MountLink`，application 内不再有别的依赖。

- 新增 `crates/agentdash-spi/src/connector/capability_delta.rs`，承载上述类型与计算函数；`connector/mod.rs` + `lib.rs` 导出。
- `session/capability_state.rs` 删除这些类型/函数的本地定义，改 `pub use agentdash_spi::{...}` 透传（`session/mod.rs` 的 `crate::session::CapabilityStateDelta` 等路径不变，全部消费方零改动）。`link_key` 因仍被 `event_payload` 使用而保留在 application。
- `cargo check --workspace` 通过；`cargo test -p agentdash-application --lib` **604 passed**（基线一致）；`agentdash-api` 88 passed，唯一失败是先前既存且无关的 `vfs_access::tests::runtime_tool_schemas_are_openai_compatible`（fs builtin schema，明确不归本任务）。

### 未做项及理由

- **未合并 `surface.vfs` / `context_projection.vfs`**（姊妹任务期望本任务一并做）：调查确认二者不是死镜像（死镜像 `projections.context` 已在姊妹任务删除），而是**消费方语义不同的读模型** —— `surface.vfs` 喂 launch executor（orchestrator/planner/plan/extension_runtime/canvas tools），`context_projection.vfs` 喂 7 个 query DTO 路由（acp_sessions/canvases/project_sessions/story_sessions/task_execution/terminals/vfs_surfaces）。collapse 成"单存储+派生访问器"需改动 launch 链 + 7 个 `agentdash-api` 路由调用方 + finalize 同步 + `validate_for_launch`，属高 blast-radius 的命名/读模型重构，无行为收益。`validate_for_launch` 的 `capability_state.vfs.active == surface.vfs` 等断言是真实漂移防护，非 slop。姊妹任务已对此项做过收益/风险评估并判定不利，本任务确认并维持该判断。

### 建议人工 review

---

## 🔴 wave2 重审（reopen 2026-05-29）

**为何 reopen**：wave2 盲审复现"Capability/Delta 多处建模"，并点名一处前轮**未触及**的具体重复。本任务上方「调查结论」对 trait-merge 给出了**正交职责**的具体证据（effect-application 轴 vs render 轴，无同名方法、输入输出不同、维度集仅部分重叠）——按铁律对抗式复核，非预设其错。

**两条独立线：**

### 线 1 · 复核 trait-merge 争议（验证正交性论证）
1. 实读 `CapabilityDimensionModule`（`capability_state.rs:248`）与 `DimensionDelta`（`dimension/mod.rs:19`）的方法签名与维度集，独立确认是否真正交（无同名方法、输入 effect-record vs 计算后 delta、维度集 companion vs capability_key/tool_path/skill/tool_schema）。
2. 若属实 → 确认无需合并，新证据逐条入 journal（不得仅引用前轮）。
3. 若发现实为可合并 → 合并为单一 trait，impl 不得堆 `unimplemented!`。

### 线 2 · 收掉前轮未碰的具体重复（盲审新点名，无条件做）
- 盲审：`hooks::CapabilityDelta {added, removed}`（`spi/hooks/mod.rs:457`）与 `connector::SetDelta {added, removed}`（`spi/connector/capability_delta.rs:14`，前轮刚上移到此）**同 crate、结构完全相同、两个名**。合并为 `SetDelta`，删 `CapabilityDelta`（零下游风险）。
- 线 1 的 `surface.vfs`/`context_projection.vfs` 单存储派生：前轮在此任务与 `session-assembly-converge` 之间**互相推卸、两边都没做**。本轮归属到 `session-assembly-converge` 线 2 执行（见该 prd），此处不重复，但须在 journal 交叉确认已落地。

### wave2 硬验收（替代上方旧 Acceptance）
- [x] `rg "struct CapabilityDelta|enum CapabilityDelta" crates/agentdash-spi/src/hooks` = **0**（已并入 `SetDelta`）
- [x] 线 1 trait-merge 结论入 journal：合并（单 trait grep 命中）或逐条新证据确认正交
- [x] delta 纯数据类型仍在 spi（前轮已做，回归确认不退回 application）
- [x] `cargo check --workspace` + capability/session 测试不回归（基线 604）
- [x] 任何缩窄逐条入 journal 标"建议人工复核"

### wave2 实施结果（2026-05-30）

- `hooks::CapabilityDelta` 已删除，`agentdash_spi::hooks` re-export `SetDelta`；hook runtime、step activation、capability notification 与 session transition 统一消费 `SetDelta`。
- `SetDelta::compute(old, new)` 承接旧 capability key diff 行为，JSON shape 仍为 `added` / `removed`。
- 复核结论维持：`CapabilityDimensionModule` 与 `DimensionDelta` 分别服务 effect replay 与 render/projection，不合并；`surface.vfs` / `context_projection.vfs` 单存储派生继续归 `session-assembly-converge`。
- 验证：`rg "struct CapabilityDelta|enum CapabilityDelta" crates/agentdash-spi/src/hooks` 无命中；`rg "CapabilityDelta" crates/agentdash-application/src/session crates/agentdash-spi/src/hooks` 无命中；`rg "CapabilityDelta" crates` 无命中；`cargo check --workspace` 通过。
- 指定测试 `cargo test -p agentdash-application --lib capability` 与 `cargo test -p agentdash-application --lib session::capability` 仍因既存 test-only persistence mock 返回 `std::io::Error`、未同步 `SessionStoreError` 而无法编译；该债务已记录在 wave2 总 checklist 的全局 gates 中，非本轮 `SetDelta` 合并引入。
