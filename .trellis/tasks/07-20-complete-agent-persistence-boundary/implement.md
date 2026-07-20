# Complete Agent 持久化边界收敛实施计划

## Preconditions

- 本子任务依赖父任务 `07-20-database-persistence-boundary-cleanup` 的 Persistence Decision Rule。
- 实现前读取 `prd.md`、`design.md`、`research/current-model-evidence.md` 与 JSONL 中的全部规范。
- 工作区可能存在并行修改；每个实现或检查代理只能修改本任务明确涉及的文件，不能回退其它改动。
- 项目未上线，本计划采用 schema hard cut；不实现兼容层或 fallback。

## Phase 1 — Lock the Regression and Contracts

- [ ] 在 Complete Agent Host public seam 增加红测：同一逻辑 Codex instance 分别 attach 到两个 Host
  incarnation，旧模型触发 exact 用户错误。
- [ ] 增加 stale dispatch 红测：若只替换相同逻辑 instance 的 live handle，旧 binding 必须拒绝而
  不是命中新 handle。
- [ ] 在 `agentdash-agent-service-api` / Host contract 中引入 opaque live attachment identity 与
  exact durable binding target。
- [ ] 更新 binding、Runtime target、provision/recovery request/result 和 callback/event envelope
  的坐标，使 attachment/incarnation 成为显式 fence。
- [ ] 运行 contract/Host 定向测试，确认新类型的 identity owner 唯一。

## Phase 2 — Build the Process-local Live Catalog

- [ ] 将 descriptor、verification、offer、placement、remote mapping 与 live handle 收敛进一个
  process-local Live Complete Agent Catalog module。
- [ ] 将 `register_verified_service` 重构为 attach/ensure 语义；不写 Host repository。
- [ ] catalog 以 exact attachment ID 解析 service，删除按逻辑 instance ID 的 dispatch fallback。
- [ ] Codex 启动 registration 改为 attach；materialization/describe 失败形成当前诊断并继续启动。
- [ ] Native selector 改为按 Product profile/provider/credential scope attach/ensure，验证同
  incarnation 幂等。
- [ ] Runtime Wire admission 使用 connection epoch 产生 attachment，disconnect retire。
- [ ] 增加 catalog 单元测试和 static/dynamic/remote composition tests。

## Phase 3 — Refactor Durable Host and Recovery

- [ ] 从 `CompleteAgentHostFacts` 删除 live inventory maps。
- [ ] provision/register binding 只接受 Live Catalog 返回的 exact selection，持久化 target snapshot。
- [ ] dispatch、effect、callback、lease 与 source validation 加入 attachment/incarnation fence。
- [ ] runtime target recovery 选择当前 compatible live attachment 并提升 generation。
- [ ] 旧 attachment、generation、callback route、lease 与 late event 全部 typed reject。
- [ ] 保留 effect identity/receipt/inspection monotonicity；增加跨 restart inspect/reconcile 测试。
- [ ] 更新 product provisioner、recovery planner 与 exact/remote selection catalog 调用链。

## Phase 4 — PostgreSQL Hard Cut

- [ ] 列出所有指向 Complete Agent inventory/Host binding 表的 FK、repository writer 和读取路径。
- [ ] 新增下一序号 migration，按依赖顺序清理错误模型下的 Product/Host development facts。
- [ ] 删除全局 inventory 表；重建或调整 target/binding/effect/source/callback/lease/provisioning/
  recovery 表以保存 exact target snapshot。
- [ ] 更新 PostgreSQL Host repository encode/decode/commit 和 normalized table projection。
- [ ] 更新 embedded PostgreSQL repository tests、schema snapshot/guard 与 migration tests。
- [ ] 验证 canonical Managed Runtime journal/projection、Provider 配置与 Product execution profile
  表未被清理。

## Phase 5 — Discovery and Diagnostics

- [ ] execution profile route 注入 Live Catalog availability projection，不再读取 Host repository
  inventory。
- [ ] `PI_AGENT` 按 Product profile + Provider catalog 判断；`CODEX` 按当前 attachment/diagnostic
  判断；unknown typed reject。
- [ ] optional adapter failure 保持当前应用启动隔离，但完整性错误只隔离 attachment 且产生结构化
  diagnostics。
- [ ] 更新 API route、AppState composition 和相关前端契约或测试；若 DTO 不变则验证无需重生成。

## Phase 6 — Delete Superseded Model

- [ ] 删除旧 service instance/verification/offer/placement repository interface、类型、SQL 与测试。
- [ ] 删除仅为 durable inventory 服务的 discovery helper 和 recovery profile 缓存路径。
- [ ] 全仓搜索确认不存在按逻辑 instance ID 解析 live handle、旧 inventory 表名或旧 maps。
- [ ] 同步 `.trellis/spec/backend/agent-runtime-driver-host.md`、persistence/facade 规范，记录最终
  authority 和 restart fencing 原因。

## Validation

按小到大运行，避免无意义重复：

```powershell
cargo test -p agentdash-agent-runtime-host <new-live-catalog-and-restart-tests>
cargo test -p agentdash-infrastructure <complete-agent-repository-tests>
cargo test -p agentdash-api execution_profiles
cargo check -p agentdash-agent-runtime-host -p agentdash-infrastructure -p agentdash-api
node scripts/check-migration-history.js
pnpm run contracts:check
```

数据库验证：

```text
使用隔离 data root 运行 migration/repository tests；
验证当前开发 schema 到新 schema 的顺序升级；
验证空 data root 得到相同最终 schema。
```

最终进程反馈环：

```powershell
pnpm run dev:server
# 停止后，用同一 data root 再运行一次
pnpm run dev:server
```

最终质量门：

```powershell
cargo clippy -p agentdash-agent-runtime-host -p agentdash-infrastructure -p agentdash-api --all-targets -- -D warnings
cargo test -p agentdash-agent-runtime-host -p agentdash-infrastructure -p agentdash-api
git diff --check
```

格式化使用受影响 crate 的 Cargo/rustfmt。若 workspace reference checkout 使 `cargo fmt --all` 无法
解析，按 `AGENTS.md` 使用同 toolchain `rustfmt --edition 2024 <changed-files>`，不得为本任务修改
reference 配置。

## Review Gates

1. **Contract gate**：binding/target 已显式包含 attachment/incarnation，旧 handle lookup 已删除。
2. **Persistence gate**：live inventory 不出现在 Host facts、SQL 表或 discovery 事实源。
3. **Recovery gate**：新 generation + 新 attachment；旧 command/event/callback/lease 全面 fenced。
4. **Effect gate**：跨重启不重复 dispatch，Unknown/InspectionRequired 保留。
5. **Migration gate**：现有开发库和空库得到同一最终 schema，canonical Runtime/Product 配置保留。
6. **Process gate**：同一 data root 连续两次启动成功。

## Risk and Recovery Points

- 类型与 schema 改动必须先通过 contract/Host 测试再落 migration，避免数据库先进入无法由当前代码
  读取的状态。
- migration 是开发态 hard cut；失败时修正 migration/代码并在隔离 data root 重跑，不增加旧 schema
  compatibility。
- 若发现某项“live inventory”确实满足父任务 Persistence Decision Rule，必须退回 planning，
  更新 PRD/design 后再实现，不能临时保留第二事实。
