# 架构最终彻底收口实施计划

## Phase 0: Preflight

- 确认工作区干净或只包含本任务改动。
- 读取本任务 `prd.md`、`design.md`、`implement.jsonl`、`check.jsonl`。
- 刷新相关 spec：前端 type safety、cross-layer contracts、hook guidelines、quality guidelines、workspace/backend 架构。
- 按工作线派发并行实现，但保持提交分组。

## Phase A: Quality Gate CI Adoption

### Files To Inspect

- `scripts/lib/quality-gates.js`
- `scripts/quality-gates.js`
- `scripts/lib/quality-gates.test.js`
- `package.json`
- `.github/workflows/pr-quick.yml`
- `.github/workflows/deploy-contract.yml`
- `.github/workflows/heavy-check.yml`

### Work Items

- 让 manifest runner 支持 CI 需要的 `run <gate>` 行为与可诊断输出。
- 将 PR quick、deployment contract、full local 等 workflow/root script 检查命令切到 manifest runner。
- 保留 workflow 的 setup/cache/artifact 编排，移除重复检查命令清单。
- 补充或更新 quality gate 单元测试。
- 更新质量门相关 spec。

### Verification

- `pnpm quality:gates:test`
- `pnpm quality:gates:check`
- 针对改动的 workflow/script 做 manifest 展开或 dry-run 验证；如实际 gate 成本合理，运行对应 gate。

## Phase B: Generated Runtime Validators

### Files To Inspect

- `crates/agentdash-contracts/src/contract_generation.rs`
- `crates/agentdash-contracts/src/generate_ts.rs`
- 生成 DTO 输出目录
- `apps/app-web/src/features/*/stream*`
- `apps/app-web/src/shared/*validator*`
- 相关 contract generation tests 与 app-web tests

### Work Items

- 确认当前 TS 生成器如何表示 tagged union、struct、nullable/list/map 等类型。
- 为实际 NDJSON envelope 生成运行时 validator 或 validator metadata。
- 让前端 stream parser 消费生成校验能力，保留协议级错误上下文。
- 删除或收束重复手写 shape 判断。
- 补充生成器测试、生成物一致性测试、前端流解析测试。
- 更新 cross-layer contract 与 frontend type-safety spec。

### Verification

- `cargo test -p agentdash-contracts`
- `pnpm contracts:check`
- 相关 app-web stream parser 单元测试
- 必要时运行 `pnpm --filter app-web typecheck`

### Implementation Record

- Workstream B 采用 contract generator 内的小型 NDJSON validator schema AST，生成 `packages/app-web/src/generated/ndjson-stream-validators.ts`。这样 stream branch、字段必填性、JSON object payload 与 cursor 字段校验由 `agentdash-contracts` 输出，并被 `pnpm run contracts:check` 覆盖。
- `Session` 与 `Project` 前端 validator 只把 generated failure 映射为 stream-local `Error`，并保留 `SessionEventEnvelope` view-model 投影；transport 不再持有分支 shape 判断。
- `notification: BackboneEnvelope` 做最小结构校验（`event/sessionId/source/trace/observedAt` 存在），原因是完整 Backbone event union 已由独立 Backbone protocol 生成类型和 TypeScript 编译约束；在 NDJSON validator 中深拷一套 Backbone 运行时 union 会制造第二个大型协议事实源。

## Phase C: WorkspaceModule Pure Outcome

### Files To Inspect

- workspace module agent surface 相关 Rust 模块
- workspace module tool adapter 相关 Rust 模块
- workspace module surface/tool tests
- `.trellis/tasks/archive/2026-06/06-30-workspace-module-agent-surface/design.md`

### Work Items

- 定义不依赖 `AgentToolResult` 的领域化 operation outcome。
- 将 invoke/present 分支改为返回领域 outcome。
- 在工具 adapter 内集中完成 `AgentToolResult` 投影。
- 调整测试：surface 测 outcome，adapter 测 protocol projection。
- 删除 surface 层不再需要的 SPI 依赖。
- 更新 backend/workspace module 或 cross-layer spec。

### Verification

- workspace module 相关 `cargo test`
- `cargo check` 覆盖受影响 Rust crate
- 针对 invoke/present 分支的 focused tests

## Phase D: AgentRun Control-Plane Direct Tests

### Files To Inspect

- `apps/app-web/src/features/agent-run-workspace/**`
- `apps/app-web/src/features/agent-run-workspace/hooks/**`
- `apps/app-web/src/features/agent-run-workspace/__tests__/**`
- `.trellis/tasks/archive/2026-06/06-30-agentrun-workspace-control-plane/design.md`

### Work Items

- 找到 control-plane command state 的最小直接测试面。
- 必要时提取纯 projection/model 函数，让 hook 只负责接线。
- 覆盖 refresh、submit、cancel、promote、presentation、禁用态和错误态。
- 保留现有 page walkthrough 作为用户路径覆盖。
- 更新 frontend hook/type-safety spec。

### Verification

- 相关 app-web 单元测试
- `pnpm --filter app-web typecheck`
- 如 UI 接线有变化，再运行相关页面级测试

## Phase Final: Convergence

- 汇总四条线的验证结果。
- 运行适合本次改动面的 Trellis check 或 quality gate。
- 确认提交按工作线分清楚。
- 更新任务实施记录与检查记录。
- 完成后归档任务。
