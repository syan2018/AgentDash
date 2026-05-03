# ACP API 层 Facade 契约

## Goal

定义 ACP 作为 HTTP API 层唯一的外部协议转换 facade，明确 backbone 事件 → ACP 的转换规则和边界，使 ACP 完全退出内部链路。

## 背景

当前 ACP `SessionNotification` / `SessionUpdate` 贯穿全链路：

```
connector → [ACP 类型] → session hub → persistence → WebSocket → 前端
```

目标架构：

```
connector → [BackboneEvent] → session hub → persistence → WebSocket → 前端
                                                               ↓
                                               HTTP API (/api/acp/*) → [ACP 类型] → 外部客户端
```

ACP 只在最外层 API 路由做协议转换，内部链路全部使用 backbone 事件类型。

## 工作内容

### 1. 确定 ACP facade 方法面

保留最小方法集：
- `session/new` — 创建会话
- `session/prompt` — 发送提示
- `session/update` (SSE) — 事件流（backbone → ACP 实时转换）
- `session/cancel` — 取消执行
- `history/read` — 读取历史

### 2. 定义 BackboneEvent → ACP 转换规则

| BackboneEvent 类型 | ACP SessionUpdate 映射 |
|---|---|
| CodexNotification(AgentMessageDelta) | AgentMessageChunk |
| CodexNotification(ReasoningTextDelta) | AgentThoughtChunk |
| CodexNotification(TokenUsageUpdated) | UsageUpdate |
| CodexNotification(TurnCompleted) | SessionEnded / Error |
| CodexNotification(ItemStarted/Completed) | AgentMessageChunk (文本描述) |
| CodexNotification(CommandExecutionOutputDelta) | AgentMessageChunk (文本) |
| CodexNotification(FileChangeOutputDelta) | AgentMessageChunk (文本) |
| PlatformEvent(*) | SessionInfoUpdate |

### 3. 内部链路 ACP 退出路径

逐步替换，以 crate 为单位：

1. `agentdash-spi` — `ExecutionStream` 类型签名替换
2. `agentdash-executor` — connector 层直接产出 BackboneEvent
3. `agentdash-application` — session hub / persistence 消费 BackboneEvent
4. `agentdash-api` — WebSocket handler 推送 BackboneEvent；ACP 路由做转换
5. 前端 — 消费 backbone TS 类型

## Acceptance Criteria

* [ ] ACP facade 转换逻辑集中在 `acp_sessions` 路由模块
* [ ] 内部链路不再引用 `agent_client_protocol` 的 Session 类型
* [ ] 外部 ACP 客户端可通过 facade 正常工作
* [ ] 转换规则文档化

## Dependencies

* 依赖 `backbone-event-model` 完成
* 依赖 `rust-ts-protocol-binding` 完成（前端类型可用后才能替换 WebSocket 推送）

## Out of Scope

* 不扩展 ACP 方法面
* 不做跨执行器的 ACP 兼容层
