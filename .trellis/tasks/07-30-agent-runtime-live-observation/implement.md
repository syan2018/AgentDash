# 实施计划

## 总体策略

按可编译的纵向切片推进，不保留新旧双协议。每一步结束时相关 crate/tests保持可运行；
下一步直接迁移全部调用方并删除旧入口。

## Phase 1：合同与门禁

- [x] 在 `agentdash-agent-runtime-contract` 抽取 `AgentObservationState`。
- [x] 将 `AgentLiveEvent` 替换为原子 `AgentLiveBatch`。
- [x] 将浏览器更新合同替换为 `AgentRuntimeStreamFrame::{Baseline,Update,ResetRequired}`。
- [x] 更新 schema/codec/TypeScript generation roots与 manifest。
- [x] 增加负向门禁：live update路径不得调用 `CompleteAgentService::read`。
- [x] 更新 contract round-trip、unsafe u64、union与 malformed payload tests。

验证：

```powershell
cargo test -p agentdash-agent-runtime-contract
cargo test -p agentdash-agent-runtime-wire
pnpm contracts:check
```

## Phase 2：Native snapshot/live owner

- [x] `DashHistoryCallbacks::committed` 把 exact suffix + folded state作为一个 batch发布。
- [x] `DashExecutionCallbacks` 只发布 ephemeral presentation batch。
- [x] Runtime observation/gateway移除 per-event `read_view`。
- [x] source mismatch、lag与callback projection error变为 typed stream reset/error。
- [x] 增加 read-count spy：100 个 delta不触发 authoritative read。
- [x] 增加旧 waiting排队后 terminal commit的顺序组合测试。

验证：

```powershell
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-agent-runtime
cargo test -p agentdash-application-agentrun
```

## Phase 3：API 与前端增量连接

- [x] API update endpoint实现 subscribe-first baseline handshake。
- [x] 将 stream error映射为 reset frame并写 diagnostics，删除静默 `Err(_) => break`。
- [x] generated validator接收新 frame union。
- [x] `AgentRuntimeConnection` 分离 baseline/state/live update，不累积 ephemeral conversation。
- [x] `useSessionStream` 改为 baseline重建、update增量归约。
- [x] Thinking以 active turn为gate，并覆盖 terminal吸收、round/attempt identity。
- [x] 删除 terminal常规 refresh；保留 reset/command/manual refresh。
- [x] 增加长历史 update工作量和 payload边界测试。

验证：

```powershell
pnpm --dir packages/app-web test -- src/features/agent-run-runtime src/features/session/model
pnpm --dir packages/app-web typecheck
cargo test -p agentdash-api
```

## Phase 4：工具 progress 深模块

- [x] 定义 `AgentToolExecutionEvent/Stream` 和 typed progress payload。
- [x] 迁移 Core loop为 started → progress* → completed。
- [x] 迁移 Host callback broker、Runtime Tool Broker与平台 AgentTool adapter。
- [x] before-tool deny、validation、cancel、EOF/error均生成唯一 terminal。
- [x] Native canonical mapper按 owner projector生成对应 update/delta。
- [x] `fs_apply_patch` 报告拟议 patch开始态和真实文件处理进度。
- [x] command/shell、MCP、dynamic工具接入同一事件流。
- [x] 删除 result-only production callback入口。

验证：

```powershell
cargo test -p agentdash-agent-core
cargo test -p agentdash-agent
cargo test -p agentdash-agent-runtime-host
cargo test -p agentdash-application-vfs
```

## Phase 5：RuntimeWire / Remote / Codex 对齐

- [x] RuntimeWire增加 correlated tool progress frame与有序 response barrier。
- [x] Remote proxy和Local endpoint传递完整 progress序列。
- [x] disconnect/timeout在terminal前形成 lost/unavailable，不伪造成功 terminal。
- [x] Codex adapter核对 vendor progress到 canonical lifecycle的映射，确保与Native相同终态语义。
- [x] 更新 generated wire codecs与loopback tests。

验证：

```powershell
cargo test -p agentdash-agent-runtime-wire
cargo test -p agentdash-integration-remote-runtime
cargo test -p agentdash-integration-codex
```

## Phase 6：Production composition 与证据收敛

- [x] 增加 Native真实 `fs_apply_patch`、command、MCP/dynamic工具组合测试。
- [x] 增加完整 tracer：input → waiting → text → tool lifecycle → final → terminal → reload。
- [x] 增加 lag/reset/reconnect tracer。
- [x] parity tests必须调用当前 production composition，而不是只回放旧 golden mapper。
- [x] 修正 inventory、scenario catalog和不存在的 conformance test引用。
- [x] 删除旧 contracts、兼容 parser、无生产者的伪覆盖测试与死代码。
- [x] 更新相关 `.trellis/spec/`。

验证：

```powershell
pnpm contracts:check
pnpm --dir packages/app-web test
pnpm --dir packages/app-web typecheck
cargo test -p agentdash-agent-runtime-test-support
cargo test -p agentdash-integration-native-agent
cargo test -p agentdash-integration-remote-runtime
```

只运行与本次改动相关的 workspace检查；共享脏工作区中不得修改其他会话文件。

## Review Gates

- [x] 每个 live token路径都可通过调用图证明没有 authoritative read。
- [x] state revision与presentation顺序来自同一 owner发生点，不存在事后补 snapshot。
- [x] 工具 lifecycle每个 item只有一个 started和一个 terminal。
- [x] reset是恢复控制信号，不是 conversation event。
- [x] 前端控制态只来自 `AgentObservationState`。
- [x] production composition缺少 progress接线时测试必须失败。
- [x] specs、inventory与源码合同一致。

## 风险文件与回退点

本项目不保留兼容实现。实施时以每个 Phase前的提交作为开发回退点，但最终代码只保留新合同。
高风险区域：

- generated contract与wire codec闭包；
- Native commit callback的原子 batch边界；
- RuntimeWire callback correlation；
- React connection baseline/update竞态；
- apply_patch部分失败与最终实际结果的一致性。

若发现需要新增 Runtime/Product持久化、改变 concrete Agent owner或数据库 schema，停止实施并退回
规划；这代表设计边界发生变化，不能作为当前方案的隐式扩张。
