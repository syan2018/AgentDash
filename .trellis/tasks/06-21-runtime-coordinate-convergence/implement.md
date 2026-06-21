# Runtime Coordinate 执行计划

## Phase 1: Design Lock

- [ ] 定义 AgentRun current delivery binding 字段与持久化位置。
- [ ] 定义 `DeliveryRuntimeSelectionService` 输入、输出和错误语义。
- [ ] 明确 repository raw latest API 命名与允许使用范围。

## Phase 2: Consumer Migration

- [ ] Workspace query 使用 unified selection。
- [ ] Cancel / subject execution control 使用 unified selection。
- [ ] Mailbox delivery target 使用 unified selection。
- [ ] API route-local duplicate resolver 移除或改为调用 selection service。

## Phase 3: Projection

- [ ] SubjectExecutionView 增加 execution history。
- [ ] `latest_runtime_node` 从 history 派生。
- [ ] AgentRun resource surface DTO 表达 frame/source coordinate。

## Validation

```powershell
cargo test -p agentdash-application lifecycle
cargo test -p agentdash-application agent_run
cargo test -p agentdash-domain workflow
pnpm run frontend:check
```

