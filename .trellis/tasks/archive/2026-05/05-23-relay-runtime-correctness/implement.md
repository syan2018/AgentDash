# Relay 运行时正确性修复 Implement

## Order

1. 阅读相关 spec 与代码：
   - `.trellis/spec/cross-layer/desktop-local-runtime.md`
   - `crates/agentdash-application/src/relay_connector.rs`
   - `crates/agentdash-api/src/relay/registry.rs`
   - `crates/agentdash-local/src/handlers/prompt.rs`
2. 调整 `RelayAgentConnector::prompt`：
   - 创建 channel；
   - 注册 session sink；
   - 发送 `relay_prompt`；
   - 错误和 stream 结束时清理 sink。
3. 调整 `BackendRegistry.pending`：
   - 引入 `PendingRequest { backend_id, tx }`；
   - unregister 时 drain 对应 backend pending。
4. 调整 local forwarder：
   - 增加 session forwarder registry；
   - 同一 session 已有 forwarder 时不重复 spawn。
5. 补测试：
   - response 前 notification 不丢；
   - backend disconnect pending 立即失败；
   - 多轮 prompt 不重复转发。
6. 更新 spec。

## Validation

```powershell
cargo test -p agentdash-api relay::registry
cargo test -p agentdash-application relay_connector
cargo test -p agentdash-local handlers::prompt
cargo check -p agentdash-api -p agentdash-application -p agentdash-local
```

如测试名称不同，以 `rg -n "relay_connector|BackendRegistry|forward_session_notifications"` 定位后运行对应包测试。

## Rollback Points

- Sink lifecycle 改动独立于 pending 改动；若 stream cleanup 牵涉过大，先保留注册顺序修复和失败清理。
- Local forwarder 去重可单独提交，避免影响 cloud registry 修复。
