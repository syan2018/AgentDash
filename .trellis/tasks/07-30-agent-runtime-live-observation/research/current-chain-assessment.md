# 当前观测链路评估

## 对比基线

- 当前：`4a50dc1619f0bcde5ece0c3959e64d00d7847a9a`
- 早期 reference：`957fa9d60ea3d67efa1bb278fe5b376cf0c34598`
- 差异规模：2107 files changed，402677 insertions，171445 deletions。

本评估只追踪实际生产链：

```text
Native/Core
  -> Complete Agent live event
  -> Agent Runtime observation/update
  -> API NDJSON
  -> AgentRuntimeConnection
  -> Session reducer/feed
```

## 已确认根因

### 1. 每个 live event 都触发完整 authoritative read

- `crates/agentdash-application-agentrun/src/agent_run/product_projection_gateway.rs`
  的 `CompleteAgentRuntimeUpdateStream::next` 对每条 event调用 `reconcile_live`。
- `crates/agentdash-agent-runtime/src/runtime_observation.rs:117`
  的 `reconcile_live` 调用 `read_view`。
- `crates/agentdash-integration-native-agent/src/service.rs:895`
  的 `read` 调用 `history_records(&read.history)`，重建完整 conversation。
- API随后为每个 token序列化完整 observation。

该结构由 `af458f82f` 引入，`470d26a67` 只移动了抽象归属，没有改变复杂度。

### 2. 前端再次完整合并和重放

- `agentRuntimeProjection.ts` 对完整 observation conversation逐条 `findIndex`。
- `useSessionStream.ts` 每次 records变化都重新 map全部 events，并从
  `createInitialStreamState` 开始执行完整 reducer。
- `c5d8a2b4a` 通过保留旧 ephemeral records修复“下一 update丢上一 delta”，但没有解决
  per-update完整工作量。

### 3. Thinking 可由正常顺序稳定复现

旧 provider waiting event进入 `reconcile_live` 后，读取到的 snapshot可能已经 terminal。
Runtime update因此包含：

```text
observation.conversation: terminal
presentations: older waiting
```

前端按上述顺序归约，`turn_completed`先删除 waiting，随后 waiting又被写回。
`useSessionFeed` 合成 Thinking时没有以 `runtimeView.observation.execution.active_turn` 作为权威 gate。

纯 reducer复现结果：

```json
{"waiting":[["turn-1",1]],"lastAppliedSeq":1,"lastEphemeralSeq":1}
```

### 4. Native 工具生产合同没有 progress

- `agentdash-agent-core::CoreEvent` 只有 `ToolCallRequested` 与 `ToolCallCompleted`。
- Core调用 `tools.invoke` 后只等待最终结果。
- `DashAgentCoreToolCallbacks::invoke` 只调用一次 `AgentHostCallbacks::invoke_tool` 并等待
  `AgentToolResult`。
- Native canonical mapper只生成 `ItemStarted` 与 `ItemCompleted`。
- 当前生产路径没有 `ItemUpdated`、`command_output_delta`、
  `file_change_patch_updated` 或 `mcp_tool_call_progress` producer。

早期 reference 的通用 Agent loop具备 `ToolExecutionStart/Update/End` 和独立
`ToolUpdateCallback`。早期 `FsApplyPatchTool` 自身没有使用 callback，因此已确认的回归是
“平台 progress能力缝丢失”，不能宣称早期 apply_patch已完整逐文件刷新。

### 5. lag 与 stream error不可观测

- Native broadcast容量为 1024。
- consumer落后返回 `Lagged(skipped)`。
- API runtime update route使用 `Ok(None) | Err(_) => break`，将错误静默转换为 EOF。
- 前端重连后能恢复 durable history，但无法恢复丢失的 ephemeral token/tool progress。

### 6. 测试覆盖产生错误安全感

相关 15 个前端测试全部通过，但只证明手工输入 fixture能归约：

- 没有 terminal → late waiting；
- 没有多个真实 token的 read count/交付节奏；
- 没有真实 Native tool started/update/completed；
- 没有 lag/reset；
- 没有 production tool callback progress。

`session-parity/inventory.json` 标记 `final_complete` 并声明 Native完整覆盖
`item_started_updated_completed`，但工具 golden来自 pinned Main旧 stream mapper；登记的
`native_w5_scenarios_match_main_oracle_golden_strictly` 在当前源码中不存在。

## 关键提交时间线

| 提交 | 影响 |
| --- | --- |
| `09bff1316` | 新 Core event从建立时即缺少工具 progress |
| `6e05a0f56` | 前端切到 canonical conversation完整重放 |
| `f234c9fce` | Native canonical tool mapper固定为 started/completed |
| `af458f82f` | 每个 live event读取完整 authoritative view |
| `470d26a67` | 统一 observation wrapper，保留 per-event full read |
| `c5d8a2b4a` | 保留连续 delta，但以持续 overlay合并实现 |

## Spec 漂移

当前 specs同时包含两项无法共同可靠满足的要求：

1. Complete Agent execution callback只发布尚未提交的 ephemeral delta；
2. 每条 Runtime update携带“同一次 authoritative read”的 execution/control 和 presentation。

ephemeral delta发生时没有对应 authoritative state commit；事后 read可能已经越过该事件，
因此把两者封装为一个“原子 update”会制造时间倒置。理想合同必须改为：

- state只在 owner真实状态转移时发布；
- presentation按真实发生顺序发布；
- snapshot只用于恢复，不用于给每条旧事件补当前状态。
