# 上下文压缩系统架构增强实施计划

## Phase 0. Review Gate

- [x] 用户确认 MVP 范围：平台压缩只覆盖 Pi Agent / AgentDash native runtime；Codex Bridge 保留自身内部压缩逻辑，平台只维护事件与可视化契约。
- [x] 调研 Codex session tree / rollout / fork / rollback 逻辑，并记录数据库仓储下的 lineage/checkpoint 策略。
- [x] 用户确认完整 session branch / fork / rollback 产品能力拆成后续独立任务：`.trellis/tasks/04-08-session-tree-branching`。
- [x] 用户确认 checkpoint 持久化形态：`event + repository` 双写。
- [x] 用户确认本任务实现后先用 `trellis-update-spec` 固化 session checkpoint / lineage / projection head 基础契约；完整 fork / rollback 产品语义等子任务完成后再补充。

## Phase 1. Safety Fixes

- [ ] 修改 `agentdash-agent/src/compaction/mod.rs`：空摘要、API error、stream closed 不再生成占位成功摘要。
- [ ] 为 `execute_compaction` 增加 typed failure，调用方保留原 `context.messages`。
- [ ] 增加单元测试：
  - [ ] 空摘要不替换消息。
  - [ ] bridge error 不替换消息。
  - [ ] cancel 不替换消息。
  - [ ] custom summary 仍可成功，但必须非空。

Validation:

```powershell
cargo test -p agentdash-agent compaction -- --nocapture
```

## Phase 2. Token Budget Cut

- [ ] 引入压缩预算模型：`context_window`、`reserve_tokens`、`target_input_budget`、`budget_scope`。
- [ ] 将 `find_cut_point` 从 `keep_last_n` 主导改为 token budget 主导，保留 `keep_last_n` 作为 recent tail 最小保护或策略参数。
- [ ] 增加 `AgentMessage` 粗略 token 估算 helper，优先复用现有 token / context 工具；没有准确 tokenizer 时使用明确的 conservative estimator。
- [ ] 保证 tool call / tool result 边界、未完成工具调用边界不被切断。
- [ ] 增加单元测试：
  - [ ] 超长最近消息会触发进一步 cut 或 diagnostic。
  - [ ] reserve tokens 越大，保留尾部越小。
  - [ ] tool pair 边界保持完整。

Validation:

```powershell
cargo test -p agentdash-agent compaction -- --nocapture
```

## Phase 3. Pre-provider Pressure Evaluation

- [ ] 在 `agent_loop/streaming.rs` 的 `transform_context` 后增加最终 request 估算点。
- [ ] 为 connector / runtime 增加 agent ownership 判断，只有平台拥有 canonical transcript 的 runtime 进入平台压缩路径。
- [ ] 将 hook `evaluate_compaction` 调整为 policy 输入，不再是唯一触发事实源。
- [ ] 增加 `ContextPressureDecision` 及 phase/reason/budget metadata。
- [ ] 支持 pre-provider 发现超预算后执行 compaction，并重新构建 `messages_for_llm`。
- [ ] 增加测试覆盖：
  - [ ] transform_context 注入后超预算会触发压缩。
  - [ ] 未超预算不会重复压缩。
  - [ ] 压缩失败时返回明确错误或 diagnostic，不发送污染后的 provider request。

Validation:

```powershell
cargo test -p agentdash-agent runtime_alignment -- --nocapture
cargo test -p agentdash-application compaction -- --nocapture
```

## Phase 4. Checkpoint Persistence And Restore

- [ ] 定义 `CompactionCheckpoint` domain / SPI 数据结构。
- [ ] 决定并实现持久化：
  - [ ] 扩展 `context_compacted` session platform event payload，作为 UI 审计事实。
  - [ ] 新增 `session_checkpoints` repository/table，作为 restore/fork/rollback 查询事实源。
  - [ ] 在同一成功边界中完成 event + repository 双写，任一失败都不得替换 runtime history。
- [ ] 为 checkpoint schema 加入 branch-aware 字段：`created_event_seq`、`covered_until_event_seq`、`lineage_node_id`、`base_checkpoint_id`、`status`。
- [ ] 设计并落地最小 active projection cursor，保证 rollback 后 checkpoint 查询不会越过当前模型可见 head。
- [ ] 更新 `session/eventing.rs`，成功压缩时写 checkpoint，并生成扩展后的 `context_compacted` payload。
- [ ] 更新 `session/continuation.rs`，优先使用最新 checkpoint 的 replacement projection，然后 replay suffix。
- [ ] 保留 `compaction_summary` ContextFrame，但从 checkpoint 派生 section。
- [ ] 增加测试覆盖：
  - [ ] checkpoint + suffix restore。
  - [ ] 多次 checkpoint 使用最新有效 checkpoint。
  - [ ] checkpoint 写入失败不替换 runtime history。
  - [ ] rollback / active head 之后的 checkpoint 不会被 continuation 使用。
  - [ ] continuation frame 不包含被 checkpoint 覆盖的旧历史。

Validation:

```powershell
cargo test -p agentdash-application compaction continuation -- --nocapture
```

## Phase 5. Lightweight Cleanup Strategy

- [ ] 添加 strategy trait / enum，先实现不调用 LLM 的 lightweight cleanup。
- [ ] 支持对大工具结果、旧文件读取、媒体块、重复 context frame 做结构化缩减。
- [ ] cleanup 产物进入 checkpoint diagnostics / replacement projection。
- [ ] 增加测试覆盖：
  - [ ] 大工具结果被缩减但 tool result 结构仍完整。
  - [ ] 媒体块被替换为占位说明。
  - [ ] cleanup 后低于阈值时不调用 summary compaction。

Validation:

```powershell
cargo test -p agentdash-agent compaction -- --nocapture
cargo test -p agentdash-application session -- --nocapture
```

## Phase 5b. Session Branch Follow-up Boundary

- [x] 使用现有 `.trellis/tasks/04-08-session-tree-branching` 作为后续独立 task，处理完整 session branch / fork / rollback API 与 UI tree。
- [x] 在后续 task 的 PRD 中写明依赖：`session_checkpoints`、`session_lineage`、active projection cursor。
- [ ] 明确 fork 时是否 materialize child initial checkpoint；默认建议先 materialize，以换取 child restore 独立性。
- [ ] 明确 rollback 表达方式：append rollback platform event + update active projection cursor，不删除历史事件。

## Phase 6. Cross-layer Event And Frontend

- [ ] 如扩展 Backbone DTO，更新 `agentdash-agent-protocol` 并生成 TS。
- [ ] 更新前端 context frame parser / renderer，展示 checkpoint id、strategy、phase、token before/after、retained tail。
- [ ] 失败事件如进入前端 feed，使用系统事件或 context diagnostic card 展示。
- [ ] 增加前端单元测试覆盖 compaction summary frame 新字段。

Validation:

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
pnpm test -- --run ContextFrameCard
```

## Phase 7. Quality Gate

- [ ] Rust formatting / check。
- [ ] 相关 Rust tests。
- [ ] 前端 typecheck / targeted tests。
- [ ] 若有 migration，验证 PostgreSQL / SQLite migration 顺序。
- [ ] 更新 `.trellis/spec` 中 session / hook / Backbone / bundle 相关长期契约。

Suggested commands:

```powershell
cargo fmt
cargo test -p agentdash-agent compaction -- --nocapture
cargo test -p agentdash-application compaction -- --nocapture
cargo test -p agentdash-application continuation_context_frame_uses_compacted_projection -- --nocapture
pnpm typecheck
```

## Risky Files

- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent/src/compaction/mod.rs`
- `crates/agentdash-agent-types/src/runtime/decisions.rs`
- `crates/agentdash-application/src/session/hook_delegate.rs`
- `crates/agentdash-application/src/session/eventing.rs`
- `crates/agentdash-application/src/session/continuation.rs`
- `crates/agentdash-application/src/session/compaction_context_frame.rs`
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs`
- `crates/agentdash-executor/src/connectors/codex_bridge/*`
- `crates/agentdash-agent-protocol/src/backbone/*`
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
- `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
- `crates/agentdash-infrastructure/migrations/*`
- `packages/app-web/src/features/session/model/contextFrame.ts`
- `packages/app-web/src/features/session/ui/ContextFrameCard.tsx`

## Rollback Points

- Phase 1 可独立合入，作为安全修复。
- Phase 2 可在旧 checkpoint payload 下合入，只改变 cut 策略。
- Phase 4 是持久化边界，必须在 migration 与 restore 测试通过后合入。
- Phase 6 跨层 DTO 变更必须与 TS 生成同 commit 合入。
