# 按 Codex 协议处理来源会话标题

## Goal

让 AgentDash 的 Backbone 协议原生承载 Codex thread 标题消息，并由 session 业务层统一把来源标题投影为会话标题，避免把 Codex 协议消息塞进临时 key/value 元信息通道。

## User Value

- 使用 Codex 执行器时，Codex app-server 已有的 thread title 能被 AgentDash session 列表正确显示。
- 会话标题来源可解释：用户手动标题优先，其次来源执行器标题，最后才是本地首条消息派生标题。
- Backbone 继续保持 Codex 协议超集定位，新增协议消息以 typed event 表达。

## Confirmed Facts

- Codex v2 `Thread` 包含 `preview: String` 与 `name: Option<String>`；`name` 是 user-facing thread title。
- Codex `ThreadStartResponse`、`ThreadForkResponse`、`ThreadResumeResponse`、`ThreadReadResponse` 都返回完整 `Thread`。
- Codex server notification 包含 `thread/started` 与 `thread/name/updated`。
- 当前 `CodexBridgeConnector` 只读取 `response.thread.id`，没有消费 `Thread.name` 或 `thread/name/updated`。
- 当前 AgentDash `TitleSource` 只有 `auto | user`，无法区分本地自动派生标题和执行器来源标题。
- 当前 `SessionMetaUpdate { key, value }` 是泛化平台元信息通道，不适合作为 Codex protocol message 的归宿。

## Requirements

- Backbone protocol 必须新增 typed platform event 表达来源会话标题更新，而不是复用 `SessionMetaUpdate` 的任意 key。
- CodexBridge 必须把 Codex `Thread.name` 与 `thread/name/updated` 映射为 Backbone typed event。
- 来源标题必须只作为事实输入，不重新引入 provider/LLM 标题生成。
- Session 业务层必须统一投影标题优先级：`user > source > auto`。
- 来源标题为空、纯空白、或等同于 Codex `preview` 时不应覆盖当前标题。
- 来源标题不得覆盖用户手动标题。
- 前端与持久化层必须识别新的 `title_source = source`。
- 如果改动持久化枚举值，PostgreSQL 与 SQLite 的解析/写入都要同步。

## Acceptance Criteria

- [ ] `agentdash-agent-protocol` 暴露 typed title event，并能生成/编译 TS binding。
- [ ] CodexBridge 在 `thread/start`、`thread/fork` 响应中消费 `response.thread.name`。
- [ ] CodexBridge 能处理 `thread/name/updated` notification。
- [ ] Application 层收到来源标题事件后写入 `SessionMeta.title` 与 `title_source = source`，并广播统一的 `session_meta_updated`。
- [ ] `TitleSource::User` 时来源标题不会覆盖用户标题。
- [ ] 无来源标题时仍使用本地 `derive_session_title()` 生成 `auto` 标题。
- [ ] 来源标题到达晚于 `auto` 标题时，可覆盖 `auto`，但不能覆盖 `user`。
- [ ] `cargo test` 覆盖标题优先级与 CodexBridge notification 映射关键路径。
- [ ] `cargo check -p agentdash-api` 通过。

## Out Of Scope

- 不恢复 LLM/provider 标题生成器。
- 不要求本轮实现 AgentDash 用户手动标题反向同步到 Codex `thread/name/set`。
- 不新增数据库字段；只允许扩展现有 `title_source` 枚举值语义。
