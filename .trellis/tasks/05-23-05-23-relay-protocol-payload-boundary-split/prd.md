# Relay protocol payload 域拆分

## Goal

将 relay protocol payload 按 handshake、prompt、workspace、tool、mcp、terminal、session event、capabilities 拆分，保持 JSON wire format 不变。

## Requirements

- 将 `agentdash-relay/src/protocol.rs` 的 payload 类型按 handshake、prompt、workspace、tool、mcp、terminal、session_event、capabilities 拆分。
- 顶层 `RelayMessage` wire enum 与 JSON 字段名保持不变。
- 保持 public re-export 或 facade，让 api/local 调用方不直接绑定子模块内部路径。
- 每批移动后运行 `cargo check -p agentdash-relay -p agentdash-api -p agentdash-local` 和 relay protocol tests。

## Acceptance Criteria

- [ ] 至少一个 relay payload domain 拆出独立文件。
- [ ] JSON wire format 不变，现有 protocol roundtrip 测试通过。
- [ ] `protocol.rs` 只保留顶层 enum、facade 或跨域 glue。
- [ ] docs/spec 记录 relay 子协议边界原因。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
