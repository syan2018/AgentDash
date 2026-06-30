# 架构二三档收敛跟踪 - Design

## Module Strategy

本任务不是一个新产品功能，而是一组架构收口工作。设计目标是为每个二三档候选找到更深的 module interface，让 implementation complexity 集中在一个可测试 seam 后面。

## Work Groups

### A. Contract Generation Test Surface

目标 module：`agentdash-contracts` 内的 contract generation 核心。

当前 seam 是 `generate_contracts_ts` CLI 写入 generated 文件并通过 check mode 比较整棵输出树。目标 seam 是纯 generation module：

```text
DomainManifest + TypeDeclarations -> GeneratedFileSet
```

CLI 只保留 file-system adapter、真实 type export 和 check mode adapter。dedup、import、header、common type reference 等规则进入可单测 implementation。

### B. Runtime Snapshot Generated Contracts

目标 module：backend/runtime browser-facing contract。

第一阶段聚焦 backend runtime summary，将 route-local DTO 与前端手写 mirror 收进 generated contract。desktop local runtime snapshot 暂不直接搬 raw Rust struct，而是先定义 stable diagnostics DTO，避免把 supervisor implementation state 固化为 wire interface。

### C. Task Tool Local Deepening

目标 module：Task plan Agent-facing workspace。

当前 `task_read` / `task_write` 同时承担 scope resolution、use case、projection 和 JSON adapter。目标拆分：

```text
ExecutionContext -> AgentRunTaskScopeResolver -> TaskPlanScope
TaskPlanWorkspace.read(scope, query)
TaskPlanWorkspace.apply(scope, changeset)
AgentTool adapter -> serde/schema/result projection
```

### D. NDJSON Validator Exploration

目标 module：contract-derived runtime validator。

如果推进本组，先从 Project stream 或 Session stream 选择一个最小试点。目标是让 stream transport 消费 contract validator，而不是每条流手写 discriminant / required field guard。

### E. Quality Gates And Route Shim Follow-up

目标 module：quality gate manifest 与 feature command/query interface。

本组优先级低于 A/B/C。推进前需要先确定是否引入 `scripts/lib/quality-gates.js` 这类可测试 gate manifest，或先从 Canvas/Session service route shim 中提 feature command/query interface。

## Boundaries

- 本任务只做二三档收口；第一档两个大 interface 另行设计。
- 不创建 Trellis child task；并行通过本任务内 work group 分配。
- 不保留兼容性 fallback；contract 改动应同步 Rust、generated TS、前端调用方和测试。

## Validation Shape

- Contract work：`pnpm run contracts:check`、`cargo test -p agentdash-contracts`、相关 frontend typecheck。
- Runtime DTO work：`pnpm run contracts:check`、`pnpm --filter app-web typecheck`，必要时补 route mapping test。
- Task tool work：相关 Rust unit tests，必要时 `cargo test -p agentdash-application task`。
- Quality gate work：新增 node/Rust 小测试后再纳入根验证。

## Rollback Notes

每个 work group 应保持能单独回滚：A 不应同时改 runtime DTO；B 不应同时改 Task tool；C 不应改 generated contract。若并行推进，按文件范围隔离。
