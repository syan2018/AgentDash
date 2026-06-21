# Control Surface 命令边界收敛

## Goal

拆分 lifecycle create / continue 语义，统一 command taxonomy，并让 ConversationSnapshot、Extension invocation、Relay、Terminal 的控制面边界可独立演进。

## Scope

- Lifecycle start vs drain / continue public command。
- Relay command target taxonomy：`execution-placement-bound`、`session-route-bound`、`mount-utility-bound`、`setup-bound`。
- ConversationSnapshot command availability core resolver。
- Extension panel / workspace module / RuntimeGateway backend target resolver。
- Terminal 与 execution lease 的产品语义。

## Context

当前 `LifecycleDispatchService::start_lifecycle_run` 只创建 `LifecycleRun + OrchestrationInstance`，entry node 保持 Ready；但 public `POST /lifecycle-runs` 会立即调用 `OrchestrationExecutorLauncher::drain_ready_nodes`。这让 create run 与调度执行在同一 API 中耦合。

## Open Decisions

- 用户点击“开始”时，产品语义是否等价于 create + continue 的组合命令。
- public API 是否拆为 create Ready run + explicit continue/drain command。
- Terminal 是 mount utility，还是 session execution surface。
- route policy 是否必须消费 command availability core，而不是重建完整 UI snapshot。

## Acceptance Criteria

- [ ] `design.md` 定义 command taxonomy 与 lifecycle command shape。
- [ ] `work-items/index.md` 覆盖 D04、D08、D09、D10、D18。
- [ ] 可执行任务明确哪些 command 依赖 Runtime Coordinate selection。
- [ ] Lifecycle start/drain 的后续实现验收包含 Ready run 可观察性。

