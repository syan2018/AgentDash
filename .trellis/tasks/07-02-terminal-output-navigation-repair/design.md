# 终端输出展示与跳转修复设计

## Architecture

终端输出仍以 Backbone `PlatformEvent::TerminalOutput` 为跨层事实，不进入聊天 feed。修复点不是改变事件语义，而是补齐 terminal display projection：

```text
local PTY / pipe bytes
  -> relay terminal output/state
  -> cloud session eventing / Backbone PlatformEvent
  -> frontend history/live terminal projector
  -> useTerminalStore
  -> workspace terminal/output tab
```

PowerShell 对象输出的边界保持在真实 shell 进程内：PowerShell 将对象格式化为控制台文本，AgentDashboard 只采集 stdout/stderr/PTY 字节流。Codex 参考可借鉴的是 UTF-8 output preparation 和字节流传输模型，不是对象序列化。

## Terminal Projection

- live path 继续使用 `dispatchSessionPlatformEvent`，避免 terminal output 进入 reducer 后重复写入。
- history hydrate 增加显式 terminal history projector。它消费历史 page 中的 terminal platform events，将 output/state 顺序写入 terminal store。
- projector 必须幂等。建议按 `sessionId + event_seq` 记录已投影事件，避免 reconnect、StrictMode 或分页重跑导致 output 重复 append。
- terminal state event 不能只更新已注册 terminal。若缺少 cwd/process metadata，可创建最小 terminal info，并标记 capability 为 state-only/history-restored。

## Terminal Capability

terminal store 需要区分真实 interactive terminal 与 read-only output replay：

- `interactive`: 有后端 terminal id，允许 input/resize/kill。
- `read_only_output`: 命令输出 replay 或历史 output viewer，不允许 input/resize/kill。
- `state_only`: 从历史 state event 恢复出的最小 terminal projection，后续收到真实 spawn/list projection 时可补全。

Terminal tab 根据 capability 决定是否绑定 xterm `onData` / resize POST。只读 replay 可以复用 xterm 渲染，但必须禁用输入副作用，并以标题/状态表达它不是后端进程。

## Workspace Navigation

命令卡片和 terminal jump 不直接写 `useWorkspaceTabStore`。页面层提供 open workspace panel action，内部调用 `WorkspacePanel.openTab` 并展开右栏。这样 tab 初始化、layout options、panel 展开由同一入口处理。

非 AgentRun workspace 场景若没有 open-panel action，应禁用跳转或降级为当前卡片内展开输出，不静默写全局 tab store。

## Backend Reliability

terminal input、resize、kill handler 必须匹配 relay response：

- response 类型不匹配：返回服务端错误。
- response 带 error：映射为稳定 API error。
- response 成功：返回 204。

spawn 仍走 runtime surface target 解析。实现时不得引入新的旧 Session 形态终端入口；若迁移 spawn/list，需要使用 AgentRun/workspace 所属 route 和 generated service。

## PowerShell Verification

本任务对 PowerShell 的“解决”是：确认并守住真实进程字节流边界，并用 Windows-gated 测试证明对象输出经过 PTY/PowerShell host formatting 后可见。

测试失败时优先检查：

- PTY 是否真正启动 PowerShell host。
- PowerShell output encoding 是否导致文本不可读。
- terminal output event 是否成功进入 session eventing。
- frontend history/live projector 是否写入 terminal store。

不要把修复做成前端拼字符串，也不要把 PowerShell output 转为 JSON object。

## Tradeoffs

- 不把 terminal output 放入聊天 feed，避免终端噪音污染会话主线。
- 不新建独立 scrollback API，先复用 session event backlog，减少数据库和权限模型扩张。
- 不把 command output promotion 伪装成真实 terminal，牺牲“看起来统一”的 UI，换取交互语义正确。
