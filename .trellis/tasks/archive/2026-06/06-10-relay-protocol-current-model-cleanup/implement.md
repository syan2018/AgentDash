# Implementation Plan: relay protocol 当前模型收敛

## Steps

1. 盘点 `agentdash-relay` 协议类型与 API/local 调用方，确认当前构造路径使用 `prompt_blocks`、`mount_root_ref` 与 VFS/MCP/extension runtime 消息。
2. 收紧 `crates/agentdash-relay/src/protocol/prompt.rs` 与 `tool.rs`：
   - 为 prompt/tool command payload 增加未知字段拒绝。
   - 删除 tool payload 中 legacy/default 兼容注释。
   - 将 search/list 当前执行选项改为反序列化必填。
3. 更新 relay crate serde 测试：
   - 保留当前可选字段 round trip。
   - 增加旧 `workspace_root`/缺失 `mount_root_ref`/缺失 search 当前字段的拒绝测试。
4. 重写 `docs/relay-protocol.md`，以当前消息枚举和 payload 为准描述 prompt、tool、VFS、MCP、extension runtime。
5. 运行 `cargo test -p agentdash-relay` 与 `cargo check -p agentdash-relay`。
