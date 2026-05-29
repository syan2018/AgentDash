# Wave1 快速收口 swarm（9 路并行死代码/修正/去重）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 8（散点）。类：丙。

## Goal

把盲审翻出的、**文件域互不相交、无需设计决策、纯删除/正确性修正/机械去重**的收口项打成一波，一次性开多路 subagent 同窗齐发，各自 build-gate + 独立 commit，最快清掉一批确定性债。

## 收口项（每项 = 一个独立可验证 deliverable，对应一路 subagent）

### S1 · 删 agent-protocol 死代码（crate: agentdash-agent-protocol）
- 删 `crates/agentdash-agent-protocol/src/compat/mod.rs`（~518 行）及 `lib.rs:19` 的 re-export。
- `envelope_to_session_notification` / `session_notification_to_envelope` 全工作区零调用方（仅定义 + re-export）。删后尝试从该 crate `Cargo.toml` 去掉 `agent-client-protocol` 依赖。
- 验证：`cargo check -p agentdash-agent-protocol` + `cargo check --workspace`。

### S2 · 堵 application 吞错/panic（crate: agentdash-application）
- `routine/executor.rs` 8 处 `let _ = ...repo.update(...).await;`（约 :158/:168/:182/:188/:202/:205/:210/:240）改为 `if let Err(e) = ... { tracing::error!(...) }`。
- `task/artifact.rs` 请求路径 `.expect()`（:47/:330/:332/:337 `serialize_or_fail`）改为返回 `Result`/`DomainError`，不 panic。
- 验证：`cargo check -p agentdash-application` + 相关单测。

### S3 · 修 MountCapability 变体漂移（crate: agentdash-domain + agentdash-contracts）⚠正确性 bug
- domain `MountCapability` 6 变体 `{Read,Write,List,Search,Exec,Watch}`（`common/mount_capability.rs`）；contracts 副本仅 4 变体（`contracts/src/vfs.rs:337`）静默吞掉 `Exec`/`Watch`。
- 补齐 contracts 副本 + `From` 映射（本项**仅修漂移**；副本是否该彻底删除归 `contract-pipeline-unify`）。
- 验证：`cargo check -p agentdash-contracts`。

### S4 · codex_bridge spawn 生命周期（crate: agentdash-executor）
- `connectors/codex_bridge.rs:632-758` 四个 detached `tokio::spawn`（writer/stdout/stderr/child-waiter）仅 waiter 观察 `cancel_token`；consumer 丢弃 `rx` 不 cancel 则 writer/reader 维持子进程存活 → 孤儿 `npx codex app-server`。
- 把 writer/reader 循环用 `tokio::select!` 绑 `cancel_token.cancelled()`，或把 child 句柄放进 drop guard。
- 验证：`cargo check -p agentdash-executor`。

### S5 · MCP 连接池 + result→text 去重（crate: agentdash-executor + agentdash-local + agentdash-mcp）🟠含同文件改动，单 subagent 做
- `executor/src/mcp/direct.rs:97-108` 每次 `execute()` 都 connect+cancel，全握手/拆解；`discover_mcp_tool_entries` 同样。仿 `local/src/mcp_client_manager.rs` 的 `ensure_connected` 加 `RwLock<HashMap<url, RunningService>>` 连接池。
- 三份 MCP result→text：`executor/.../direct.rs:238 render_content`、`executor/.../relay.rs:104` 内联拼接、`local/.../mcp_client_manager.rs:162 render_content` → 收一个 `mcp_content_to_text(&CallToolResult)->String`，放 `agentdash-mcp` 或共享 util，三处调用。
- 注：本项与 S4 同 crate 但分处 `connectors/` 与 `mcp/`，不冲突；direct.rs 的池 + render 在同文件，故本项整体一个 subagent。
- 验证：`cargo check -p agentdash-executor -p agentdash-local -p agentdash-mcp`。

### S6 · first-party-plugins authorize gate（crate: agentdash-first-party-plugins）
- `src/lib.rs:139-146` `authorize()` 无条件 `Ok(true)`。若 Personal 模式确为有意，则在调用点 assert/注释该不变量；`ConnectorCatalogPlugin`（:41-89）明确标注为非功能占位或移除。
- 验证：`cargo check -p agentdash-first-party-plugins`。

### S7 · 前端去重（package: app-web，与后端不相交）
- formatter 收一处（`lib/format.ts` 或 `@agentdash/ui`）：`formatBytes`(4 副本，`vfs/vfs-format.ts` 已有导出)、`formatDateTime`(×3)、`formatRelativeTime`(×3)。
- `CapabilityDirective` 三定义（`types/index.ts:89`、`types/workflow.ts:105`、generated）收敛为 generated-backed 单一来源；删 `index.ts:89` + 修双重 barrel 导出（`index.ts:560` `export *` 与 :89 冲突）。
- `isJsonValue`/`requireStringField` 停止在 `services/workflow.ts`/`services/project.ts` 重定义，改 import `api/mappers.ts`。
- 验证：`pnpm -C packages/app-web exec tsc --noEmit`。

### S8 · infra db_err 合并（crate: agentdash-infrastructure）
- 6 份 `db_err`（`extension_package_artifact_repository.rs:160`、`mcp_preset_repository.rs:175` 等）合并为 `postgres/mod.rs` 单一 helper。
- **仅机械合并签名/去重**；`DomainError::Database/Conflict` 语义变体留待 `error-model-unify`，本项不动 DomainError 定义。
- 验证：`cargo check -p agentdash-infrastructure`。

### S9 · relay registry 锁加固（crate: agentdash-api）
- `relay/registry.rs:85/120/178/184/189/195/217/233` 8 处 `RwLock().unwrap()` 在 WS 热路径毒化即 panic 级联 → 换 `parking_lot::RwLock` 或显式毒化处理。
- 验证：`cargo check -p agentdash-api`。

## 并行/互斥矩阵

| 路 | 触及 crate/包 | 与谁互斥 |
|---|---|---|
| S1 | agent-protocol | — |
| S2 | application | — |
| S3 | domain, contracts | — |
| S4 | executor/connectors | S5（同 crate 不同模块，安全） |
| S5 | executor/mcp, local, mcp | S4（安全，分模块） |
| S6 | first-party-plugins | — |
| S7 | app-web (TS) | — |
| S8 | infrastructure | — |
| S9 | api | — |

→ **S1/S2/S3/S6/S7/S8/S9 七路完全不相交可任意齐发；S4+S5 同 executor 但分 `connectors/` 与 `mcp/`，可并发但建议同批观察。共 9 路。**

## Acceptance Criteria（硬指标 + 验收命令，逐条 grep 核验记 journal）

- [ ] **S1**：`rg -n "mod compat|compat::" crates/agentdash-agent-protocol/src` = 0；`rg "agent.client.protocol" crates/agentdash-agent-protocol/Cargo.toml` = 0；`cargo check --workspace` exit 0
- [ ] **S2**：`rg "let _ = .*update\(" crates/agentdash-application/src/routine/executor.rs` = 0；`rg "\.expect\(|panic!|serialize_or_fail" crates/agentdash-application/src/task/artifact.rs` = 0
- [ ] **S3**：domain 与 contracts 的 `MountCapability` 变体计数相等（均 6）：`rg -c "Read|Write|List|Search|Exec|Watch" <两文件>` 对齐
- [ ] **S4**：`codex_bridge.rs` 中 writer/stdout/stderr 三个 spawn 循环均出现 `cancel_token`（`rg -c "cancel_token" crates/agentdash-executor/src/connectors/codex_bridge.rs` ≥ 4）
- [ ] **S5**：`executor/src/mcp/direct.rs` 出现连接池容器（`RwLock<HashMap` 或复用 manager）；`rg "fn render_content|MCP tool:" crates/agentdash-executor crates/agentdash-local` 收敛为对单一 `mcp_content_to_text` 的调用（定义点 = 1）
- [ ] **S6**：`rg "Ok\(true\)" crates/agentdash-first-party-plugins/src/lib.rs` 处或被替换为带条件判断，或邻接 `debug_assert!`/注释显式声明 Personal 不变量（journal 记录所选）
- [ ] **S7**：`formatBytes`/`formatDateTime`/`formatRelativeTime` 各仅 1 处定义（`rg -c "(function|const) format(Bytes|DateTime|RelativeTime)" packages/app-web/src` = 3 总计，均在共享模块）；`CapabilityDirective` 定义 = 1；`types/index.ts:89` 已删；`tsc --noEmit` exit 0
- [ ] **S8**：`rg -c "fn db_err" crates/agentdash-infrastructure/src` = 1（单一 helper）
- [ ] **S9**：`rg "\.unwrap\(\)" crates/agentdash-api/src/relay/registry.rs` = 0（改 parking_lot 或显式毒化处理）
- [ ] 9 路各自独立 commit，均过对应 build-gate；任何缩窄写入 journal 并标"建议人工复核"

> 核验：orchestrator 在每路 commit 前跑对应命令，输出贴 journal；S 项未达 `= 0`/`= 1` 硬值且无豁免说明者，不得标 done。

## 非目标

- 不碰 `DomainError` 定义（→ `error-model-unify`）、不删 contracts 副本本体（→ `contract-pipeline-unify`）、不重构 services mapper 层（→ `contract-pipeline-unify`）。
