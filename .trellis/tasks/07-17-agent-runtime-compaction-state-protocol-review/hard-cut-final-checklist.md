# Agent Runtime Hard Cut 最终清单

## 当前基线

- [x] 当前分支已合入 S5 受检基线、Platform Tool/Hook、Product canonical presentation 与 Relay Runtime Wire 四个生产切片（`8ca341a2` → `a6dabf47`）。
- [x] `AppState` 已组合 Product persistence、final runtime tool catalog、Product authorizer、Platform handler，并向 Complete Agent 注册 tool contribution。
- [x] Cloud / Local Runtime Wire placement 生产路径与 Relay bootstrap 已存在。
- [x] 数据库只有一个 `0084_agent_runtime_complete_agent_hard_cut.sql`，已建立 Product / Runtime / Host / Dash Agent 最终分区及 fork、companion、placement、binding、effect、outbox 等状态表。
- [x] `agentdash-executor`、`agentdash-application-hooks`、旧 application runtime gateway/session crate 已物理删除；SPI 与 extension gateway 已迁入新 owner。
- [x] 当前工作树审计基线为 `a6dabf47`；旧 Product 草稿 `909a9a23` 仅作审计参考，不整体迁入。

## 剩余 Hard Cut

### 1. 收敛 canonical Product / API / frontend consumer

- [ ] 修复 `agentdash-api/tests/agent_runtime_target_projection.rs` 的 6 个旧 contract 断点：删除 `ManagedRuntimeItemContent`、`content`、`content_digest`、`ItemUpserted` 假设，补齐 `evidence` 与 `source_binding`。
- [ ] 将 Product command / mailbox 调用统一到生成的 typed contract，确保 Product 事实只经 Application 写入，Runtime projection/change 只表达平台事实。
- [ ] 将前端 feed、terminal、workspace pending、lifecycle view 的 sequence/version 全部统一为 `RuntimeU64` wire string / 应用内 `bigint`。
- [ ] 将前端 runtime fixture、round action、session projection 与 tests 从 `content/event/compactAgentRunContext` 迁到 canonical `presentation`、typed request 与 Managed Runtime item。
- [ ] 删除 UI 中已无用途的 companion/runtime 参数与旧 service contract import。
- [ ] 门禁：`cargo check -p agentdash-api --all-targets`、`pnpm --filter app-web typecheck` 通过。

### 2. 固定真实生产 composition

- [ ] 核验 Native / Codex / Remote Complete Agent 的注册、driver、connector、Host binding 与 placement 都从同一 production composition 到达，不保留测试专用或旁路 owner。
- [ ] 核验 Tool / Hook 的 `AppState` composition 与 PostgreSQL source/effect pin；读取、恢复、回调和重连均走唯一持久化事实。
- [ ] 核验 fork / companion / compaction / recovery 从 Product command 到 Runtime、Host、Complete Agent、Dash Agent history 的闭环。
- [ ] 核验 Relay placement / Runtime Wire 已由真实 caller 使用，再删除旧 Prompt / SessionEvent relay variant 与 registry 分支。

### 3. 删除旧协议与错误 owner

- [ ] 删除 workspace module 与 application 中剩余 `RuntimeJournalFact` / `RuntimeJournalRecord` / `journal_records_after` / `append_presentation` 路径；canonical presentation 只由最终 Product/Runtime contract 表达。
- [ ] 将仍依赖 `BackboneEvent` / `BackboneEnvelope` 的真实生产 consumer 迁到 Runtime / Service / Wire / Product owner；生成代码、fixture 与测试同步迁移。
- [ ] 清除平台层仍持有 agent session/history/context/compaction 语义的 `RuntimeSession` 路径；保留 Complete Agent 内的 `AgentSession = fold(history)`。
- [ ] 收窄 `agentdash-platform-spi` 的旧 AgentTool / delegate / protocol re-export；Tool capability 仅由 Complete Agent contribution 与 Platform handler 组合。
- [ ] 将 Codex 私有协议/codegen 与共享 Runtime / Service / Wire / Product contract 明确分开。
- [ ] 删除 `agentdash-agent-protocol`、`agentdash-agent-protocol-codegen`、`agentdash-agent-types` 三个旧 crate 及所有 source/generated/test consumer。
- [ ] 从根 `Cargo.toml`、workspace dependency、`Cargo.lock` 与 `package.json` contracts script 中移除旧 crate/codegen。

### 4. 固定 schema 与持久化边界

- [ ] 审核 0084 的唯一键、revision/version 单调约束、source/projection/change/outbox 关联及 fork/companion/placement/binding/effect 不变量。
- [ ] 确保 repository 和生产 reader 只使用 0084 最终表，不再读写 0070 等历史 schema 的旧 Runtime/session/journal 表。
- [ ] 更新 PostgreSQL migration/repository tests，验证重放、幂等、并发 CAS、outbox claim 与恢复路径。

### 5. 最终删除与门禁

- [ ] `rg` 负向门禁归零：旧 crate 名、`RuntimeJournalFact`、旧 RuntimeSession owner、`BackboneEvent/Envelope`、Relay Prompt/SessionEvent、兼容 adapter/fallback。
- [ ] `cargo metadata --no-deps` 确认 workspace graph 不含三个旧 crate且无反向依赖。
- [ ] 运行受影响 crate 的定向 tests/check，再运行 workspace Rust check/test、前端 typecheck/tests、contracts check 与 migration tests。
- [ ] 通过一条真实生产链验证：Product command → Runtime operation/projection/change → Host placement/effect → Complete Agent → Dash Agent history → canonical presentation/UI。
- [ ] 更新本清单为全通过，并记录仅剩的外部环境阻塞（若有）。

## 已知定向失败基线

- `cargo check -p agentdash-api --all-targets`：仅 `agent_runtime_target_projection.rs` 的 6 个旧 projection fixture/contract 编译错误。
- `pnpm --filter app-web typecheck`：错误集中在 RuntimeU64、canonical presentation、typed Product command、旧 fixture 与旧 compact consumer。
- 旧 crate 当前仍有三个物理目录；根 workspace 与 `contracts:check` 仍直接引用旧 protocol/codegen。
- 初次负向审计仍命中 5 个 journal 路径、61 个 Backbone 文件、3 个旧 Relay variant 文件；`RuntimeSession` 和 AgentTool 的宽泛命中需按最终 owner 判定后清除真实旧路径。
