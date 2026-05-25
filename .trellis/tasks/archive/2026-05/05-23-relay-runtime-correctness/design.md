# Relay 运行时正确性修复 Design

## Boundary

本任务只处理 cloud/local relay prompt 与 session event stream 的生命周期时序。核心边界在：

- cloud connector：`crates/agentdash-application/src/relay_connector.rs`
- cloud relay registry：`crates/agentdash-api/src/relay/registry.rs`
- local prompt handler：`crates/agentdash-local/src/handlers/prompt.rs`
- cross-layer relay/local runtime spec

## Design

### Sink lifecycle

`RelayAgentConnector::prompt` 应先创建 event channel 并注册 session sink，再发送 `command.prompt`。这样本机 backend 即使在 `response.prompt` 前发送 `EventSessionNotification`，cloud 侧也已有投递目标。

清理语义集中在三类路径：

- `relay_prompt` 返回错误：立即 unregister。
- stream 收到 terminal 或 channel close：unregister。
- cancel 成功：unregister。

### Pending lifecycle

`BackendRegistry.pending` 从 `msg_id -> Sender` 扩展为包含 `backend_id` 的 pending record。backend unregister 时移除该 backend 的所有 pending sender，让等待中的 `send_command` 通过 dropped sender 立即返回 `ResponseDropped`。

### Local forwarder lifecycle

local side 需要按 `session_id` 管理 notification forwarder。已有 forwarder 时复用，避免多轮 prompt 产生多个 receiver。forwarder 结束条件由 event channel close、session terminal 或 owner service shutdown 驱动。

## Test Shape

- response 前 notification：构造 fake transport/registry，先 feed event 再 resolve prompt，断言 execution stream 收到 event。
- pending disconnect：send command 后 unregister backend，断言等待方不超时。
- forwarder de-dupe：同一 session 两次 prompt 后发送一条 notification，断言 WebSocket event channel 只收到一次。

## Spec Update

更新 cross-layer relay/local runtime 文档，记录 prompt sink 先于 command 发送、pending 归属 backend、session forwarder 按 session 唯一。
