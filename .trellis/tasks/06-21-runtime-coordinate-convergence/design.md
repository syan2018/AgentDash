# Runtime Coordinate Design

## Design Decisions To Preserve

- AgentRun delivery target 是全系统统一控制面事实，不由 workspace、cancel、mailbox、SubjectExecutionView 各自查询和构造。
- AgentRun 应持有或可唯一解析 current delivery binding；binding 指向当前 run / agent / frame / node / attempt / runtime session。
- `RuntimeSessionExecutionAnchor` 是 backlink 与历史证据，不是业务 selection owner。
- repository `latest` 类 API 只能表达 raw ordering，业务 selection 进入 application-level resolver。

## Target Model

```text
AgentRun current delivery binding
  -> runtime_session_id
  -> lifecycle_run_id
  -> lifecycle_agent_id
  -> frame_id / launch_frame_id
  -> orchestration_id / node_path / attempt
  -> delivery status and observed_at

DeliveryRuntimeSelectionService
  -> reads AgentRun binding and anchors
  -> applies explicit policy
  -> returns DeliveryRuntimeSelection
```

## Policy Surface

| Policy | Meaning | Consumers |
| --- | --- | --- |
| CurrentDelivery | AgentRun 当前控制面目标 | workspace, mailbox, cancel |
| RunScopedLatest | 同一 run 内最近 delivery 证据 | transition / diagnostics |
| LaunchPrimary | launch 时 primary anchor | history / trace baseline |
| SubjectLatestObserved | subject execution history 的 latest 派生 | SubjectExecutionView |

## Implementation Shape

- 先设计并测试 selection service，不直接大规模重写所有 consumers。
- 第二步把 workspace / cancel / mailbox 迁到 service。
- 第三步扩展 SubjectExecutionView history，并从同一 history 派生 latest。
- 第四步让 resource surface DTO 表达 surface source coordinate。

## Dependencies

- Capability exposure surface 会影响 current frame VFS 和 runtime surface 刷新，但不阻塞 delivery binding 的 owner 决策。
- Control Surface 中 cancel / command policy 的实现应依赖本任务输出的 selection service。

