# 架构：relay protocol 当前模型收敛

## Goal

清理 relay protocol legacy/default 语义，并更新协议文档为当前 payload 模型。

## Requirements

- relay protocol payload 应表达当前正确模型，不保留旧字段缺省、legacy 命名或旧形态测试。
- 如果字段是当前模型的必填语义，反序列化应 fail fast；如果确实可选，应使用当前业务命名说明可选原因。
- `docs/relay-protocol.md` 应与 `agentdash-relay` serde 类型和当前 VFS/MCP/extension runtime 消息一致。
- 启动实现前需要盘点 local/backend/application 调用方，避免只收紧协议而遗漏真实调用路径。

## Acceptance Criteria

- [ ] relay tool payload 中 legacy/default 兼容语义被删除或改写为当前模型语义。
- [ ] 拒绝旧 payload 形态的测试覆盖关键协议入口。
- [ ] relay protocol 文档不再描述过期的 `prompt/workspace_root/workspace_files.*` 旧模型。
- [ ] relay crate 与相关调用方检查通过。

## Notes

- 这是协议收敛任务，当前只作为 tracking task；不要在补齐设计前 start。
