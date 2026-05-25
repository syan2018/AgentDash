# 按 Codex 协议处理来源会话标题 - Implement

## Checklist

1. [x] 扩展 `agentdash-agent-protocol`
   - 在 `PlatformEvent` 增加来源标题 typed event。
   - 更新 compat 映射，让旧 session update 输出能携带该事件信息。
   - 生成或验证 TS binding。

2. [x] 扩展标题来源模型
   - `TitleSource` 增加 `Source`。
   - PostgreSQL / SQLite title source parse/write 支持 `source`。
   - 前端 `TitleSource` union 与 session meta update handler 支持 `"source"`。

3. [x] Application 投影
   - 在 session eventing 持久化通知时识别来源标题 event。
   - 校验空标题、`title == preview`、`executor_session_id` 是否匹配当前 meta。
   - 只在当前 `title_source != User` 时写入 `source` 标题。
   - 写入后广播统一 `session_meta_updated`。
   - 本地 `auto` 派生只对无来源标题能力的 connector 生效。

4. [x] CodexBridge 映射
   - 从 `ThreadStartResponse.thread.name` 与 `ThreadForkResponse.thread.name` 发出来源标题 event。
   - 处理 `thread/name/updated` notification。
   - 如处理 `thread/started`，避免重复事件造成无意义写入。

5. [x] Tests
   - Application eventing/title projection tests：source 覆盖 auto，不覆盖 user，忽略 `title == preview`。
   - CodexBridge notification mapping test 覆盖 `thread/name/updated`。
   - `cargo check` 覆盖 `TitleSource::Source` 在 PostgreSQL / SQLite parse/write 的编译路径。

## Validation

```powershell
cargo check -p agentdash-application -p agentdash-executor -p agentdash-agent-protocol -p agentdash-spi
cargo test -p agentdash-application source_session_title -- --nocapture
cargo test -p agentdash-executor thread_name_updated_maps_to_source_session_title_event -- --nocapture
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check
pnpm --filter app-web typecheck
```

当前前端全量 typecheck 仍存在 workflow / VFS / stream typing 基线错误；本任务相关的 `TitleSource` 与 generated Backbone 类型未产生新增定位错误。

## Risk Areas

- `persist_notification` 内部再次广播 `session_meta_updated` 时要避免递归投影。
- `executor_session_id` 绑定可能与标题事件顺序相邻，CodexBridge 应在发标题事件前先发 `ExecutorSessionBound`。
- 前端若还只接受 `auto | user`，会丢掉 `source` 标识。

## Review Gate

开始实现前确认：

- 本任务不是恢复 provider/LLM 标题生成。
- Codex protocol message 以 Backbone typed event 进入系统。
- SessionMeta 是最终标题事实源。
