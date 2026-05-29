# 架构 slop 清理 wave2：契约统一 / 错误模型 / 层瘦身 / 死代码收尾（parent）

## Goal

承接 `05-29-architecture-slop-cleanup`（第一波，已执行 11 commit、正在收尾）之后，处理一次**零讨论纯代码盲审**（2026-05-29，10 路 subagent，明确不读任何旧 review/spec 讨论、只以代码为准、排除 `references/`）翻出的、第一波**未收干净或从未纳入 scope** 的结构债。

方向不变：**骨架不动，执行收紧**——消灭"同一件事做两遍 + 没有编译器兜底"的系统性病灶。

## 定性更正（重要，2026-05-29 用户拍板）

用户指出：第一波除 workflow 外的 child 都标称"搞定待收"。**盲审又翻出来，就等于前轮没收干净**——不得把这些粉饰成"wave2 新发现"。本轮所有项按下表如实定性：

| 类 | 含义 | 处理 |
|---|---|---|
| **甲·前轮欠账** | 旧 child 开了但未交付 | **reopen 原 child 重审，不在 wave2 重复造 child** |
| **乙·列了没做** | 旧 parent prd 列了病灶但无 child 真正交付 | 进 wave2（`error-model-unify`） |
| **丙·真·新面** | 第一波从未纳入过 scope | 进 wave2（其余 child） |

### 甲类 · 前轮欠账（→ reopen 旧 child，不在本 parent 下重复建 child）

第一波 outcome 已自承的未完成项，盲审独立复现，**前轮"review 高估耦合"的开脱结论需重新核对、不自动采信**：

- `05-29-session-assembly-converge`：仅删死镜像，**未抽单一 resolver**；盲审仍见 `assembler.rs` 与 `construction_planner.rs` 两个组装引擎调同一批积木产同一 plan（~1.2k 行近重复）。
- `05-29-capability-state-unify`：仅上移 delta 类型，**未合并 trait**；盲审仍见 `hooks::CapabilityDelta` 与 `connector::SetDelta` 同 crate 同结构两个名。
- `05-29-frontend-server-state-refactor`：🟡 stage C + `activity-inspector`(1304)/`SettingsPageContent`(2014)/`SessionChatView`(1008) god-component 拆分**未完**。

> 处置建议：上述三项**重审耦合真相后 reopen 对应旧 child**。其中前端 god-component 拆分若 reopen 困难，可临时挂到本 parent 的 `structural-splits` 执行（已在该 child prd 标注重叠）。

### 与第一波**不重复**（已确认收干净，本轮排除）

`app-infra-leak-to-spi`（5 port 下沉）✅、`infra-persistence-dedup` 的 session_repository 去重 ✅、`dedup-naming-boilerplate` 命名/supertrait ✅、`frontend-server-state-refactor` A/B + workspace-layout ✅。`drop-step-lifecycle`（workflow 收口）由独立任务处理，本轮**完全不碰**。

## 已拍板决策（2026-05-29 用户确认）

1. **契约单源**：逐出 domain，`contracts` 为唯一 codegen 源（ts-rs/schemars 从 domain 移除，api/dto 提升进 contracts）。→ `contract-pipeline-unify` + `domain-purification`。
2. **sqlite**：**砸掉 sqlite 后端**，`agentdash-local` 改用嵌入式 PG（`postgres_runtime.rs`）。一刀解决 fork + schema 双机制。→ `infra-residual`（含吸收第一波 de-fork 残留）。
3. **application 拆分**：本轮**折中**——先抽 `agentdash-application-ports` 建真缝 + 内部模块按职责重排；`-session`/`-workflow` 物理拆**显式推后**（不算本轮欠账）。→ `structural-splits`。
4. **前端校验**：删 services identity-mapper，内部端点直接 `api.get<GeneratedType>()` 信任 wire，不引入运行时校验管线。→ `contract-pipeline-unify`。

## 验收硬规则（防"收口不彻底"，全 child 适用）

第一波栽在"显著下降"这类软标准上被缩窄。本轮强制：

- **每条 AC 必须可机器核验**：给出 grep/count 断言或文件存在性判定，杜绝"差不多了"。
- **零容忍项写成 `== 0`**：死代码删除、`Internal(e.to_string())`、`Json<Value>` 等给出明确目标计数（通常 0）。
- **禁止静默缩窄**：任何 scope 缩窄必须在 commit message + journal **逐项列出 + 理由**，并标注"建议人工复核"；未列出的缩窄视为未完成，不得 archive child。
- **甲类残留默认不采信"耦合被高估"**：两次独立盲审命中的项，需新证据才可缩窄。
- child archive 前由 orchestrator 跑该 child 的「验收命令」逐条核对，结果贴进 journal。

## Requirements（wave2 病灶，乙/丙类）

1. **契约双流水线（盲审病灶 1，丙）**：项目存在两套契约系统——生成的 `contracts`→前端 `generated/`（真用），与手写的 `api/dto`(9/11 Serialize-only)↔前端 `types/index.ts`（手同步、零机器校验）。`Task/Story/Workspace/Project` 走手同步路，domain 改字段不报错、前端运行时直接错。统一到单一 codegen 源 + CI `--check` gate。
2. **错误模型（盲审病灶 2 / 旧 parent 病灶6，乙）**：`DomainError` 仅 3 变体 → 158 处 `InvalidConfig(e.to_string())`；application 8 个模块用 `Result<_,String>`；api **124 处 `Internal(e.to_string())`** 泄漏 DB 原文 + 字符串嗅探救 409。建结构化错误模型。
3. **vfs 去重（病灶 D，丙）**：`VfsService` dispatch 复制 8~10 遍 + inline-fs 魔法字符串；`MountProvider` 17 方法臃肿（`watch` 无消费者）；patch executor 三份（一份死代码）；`MountError` 被 `to_string()` 抹平。
4. **api 瘦身（病灶 C，丙）**：handler 直查 repo（91 处/18 文件）；`session_use_cases/construction`(1250 行) 错放在 transport crate；`routes.rs` 701 行单表；29 处 `Json<serde_json::Value>` 无类型契约。
5. **infra 收尾（病灶 H，丙）**：Postgres 时间戳存 TEXT + 6-format 手解析 + 260 处 `.to_rfc3339()`；sqlite 用 `let _=ALTER` 即兴迁移；session SPI port 用 `io::Result` 弱类型。
6. **结构性拆分（病灶 F，丙 + 甲）**：89k `application` 巨石无内部 crate 缝、`task` 伸进 10 模块；`session/` 28k 平铺 + `memory_persistence.rs`(1491) 测试夹具混在 `src/`；前端 god-component（甲类残留）。
7. **domain 净化（病灶 G，丙）**：`ts-rs`/`schemars` 渗进 domain crate；`SessionId/StorySessionId/ChildSessionId` 三个 `=String` 假新类型。
8. **死代码/快速修正（病灶 E + 散点，丙）**：`agent-protocol/compat/`(500 行死代码)+ACP 依赖；`routine/executor.rs` 8 处吞错；`task/artifact.rs` 请求路径 panic；`codex_bridge` spawn 孤儿进程；executor MCP 每调用重连 + result→text 三份；`first-party-plugins` 无条件 `authorize→Ok(true)`；`MountCapability` 4 vs 6 变体漂移（正确性 bug）；前端 formatter 10 副本 + `CapabilityDirective` 三定义 + `JsonValue` 9 份。

## 任务图（child）

| child | 病灶 | 类 | 波次 | 风险 | 并行性 |
|---|---|---|---|---|---|
| `05-29-quickfix-swarm` | 8 | 丙 | **W1** | 低（删除/修正/去重） | **9 路文件不相交，立即齐发** |
| `05-29-error-model-unify` | 2 | 乙 | W2 | 中（类型骨架→机械铺开） | 先定型→再 fan-out |
| `05-29-contract-pipeline-unify` | 1 | 丙 | W2 | 中（codegen + 删手抄） | 设计先行→可 fan-out |
| `05-29-vfs-dedup` | 3 | 丙 | W3 | 中（自包含） | 与 W2 可并行 |
| `05-29-api-handler-thinning` | 4 | 丙 | W3 | 中 | **依赖 error-model** |
| `05-29-infra-residual` | 5 | 丙 | W3 | 中（含 DB migration） | **依赖 error-model** |
| `05-29-structural-splits` | 6 | 丙+甲 | W4 | 高（拆 crate/目录/组件） | 设计先行，**不盲并行** |
| `05-29-domain-purification` | 7 | 丙 | W4 | 中 | **依赖 contract-pipeline** 定调 |

## 执行波次与并行派发

**分支**：续用 `refactor/architecture-slop-cleanup`（不 push，留待人工 review）。
**编译基线**：开工前 `cargo check --workspace` + `pnpm -C packages/app-web exec tsc --noEmit` 须为绿。

- **Wave 1（立即并行一网打尽）**：`quickfix-swarm` 内 9 路 subagent 同窗齐发（互斥见该 child implement.md），各自 build-gate + 独立 commit。
- **Wave 2（基础定型）**：`error-model-unify`（先定 `ApplicationError`/`DomainError` 骨架，再机械替换可 fan-out）‖ `contract-pipeline-unify`（先定 codegen 单源，再删手抄可 fan-out）。两者文件域基本不相交，可并行。
- **Wave 3**：`vfs-dedup`（自包含，可与 W2 重叠）；`api-handler-thinning` + `infra-residual` 须等 `error-model-unify` 类型骨架落地。
- **Wave 4（设计先行，禁止盲并行）**：`structural-splits`（拆 crate 影响全工作区 import）→ `domain-purification`（待 contract-pipeline 决定 ts-rs 去向）。

**Gate（orchestrator 执行，subagent 不得 commit）**：每任务/每并行项完成后 `cargo check --workspace`（或前端 `tsc --noEmit`）→ 过则 `git add -A && git commit`（`type(scope): 中文信息`）→ 失败一次定向修复，仍失败回退该项、journal 记录、不污染后续波次。

**第一波教训（必须遵守）**：深逻辑/高风险项（甲类残留、`structural-splits`）执行前，subagent **先调查"实际是否如盲审所述"**；但与第一波不同——本轮对甲类的"耦合被高估"结论**默认不采信**（两次独立审都命中），需给出新证据才可缩窄，且缩窄必须在 commit message + journal 显式标注供人工复核。

## Acceptance Criteria（parent，待全部 child archive 后核对）

> 全部为硬指标；括号内为核验方式。

- [ ] 每个 commit 点 `cargo check --workspace` + `pnpm -C packages/app-web exec tsc --noEmit` exit 0
- [ ] `rg -n "mod compat" crates/agentdash-agent-protocol` 与 `rg -n "apply_patch_to_inline_files|apply_patch_to_fs" crates` 均 0 行（病灶 8）
- [ ] `Task/Story/Workspace/Project` 在 `contracts` 有 `#[derive(TS)]` 单源；前端 `types/index.ts` 对应手写 type 删除；`generate_contracts_ts --check` 进 CI（病灶 1）
- [ ] `rg "InvalidConfig\(.*to_string" crates/agentdash-infrastructure` 与 `rg "ApiError::Internal\(.*to_string" crates/agentdash-api` 计数 = 0 或附 journal 豁免清单（病灶 2）
- [ ] `VfsService` 中 `PROVIDER_INLINE_FS` 分支 ≤ 1 处；`MountProvider` 拆为 IO/Search/Descriptor 三 trait（病灶 3）
- [ ] migrations 中 `rg "created_at TEXT|updated_at TEXT"` = 0；`sqlite/` 后端目录移除（病灶 5）
- [ ] `agentdash-application-ports` crate 存在并为 workspace member；`task` 模块未新增对兄弟 concrete 模块的直依赖（病灶 6）
- [ ] `rg "ts-rs|schemars" crates/agentdash-domain/Cargo.toml` = 0（病灶 7）
- [ ] 甲类残留三项：已 reopen 原 child（`session-assembly-converge`/`capability-state-unify`/`frontend-server-state-refactor`）并在其 journal 给出"耦合"重审结论
- [ ] 各 child acceptance 全满足（逐条 grep 核验记入 journal）+ 最终跨 child 集成 review

## Notes

- 盲审原始 10 路结论保存在本次会话；**未写入 docs/**，避免与第一波 `docs/reviews/2026-05-29-slop-cleanup-review/` 混淆。如需固化另行决定。
- parent 保持 planning，待全部 child archive 后做集成 review 再归档（遵循"父任务不要早归档"）。
