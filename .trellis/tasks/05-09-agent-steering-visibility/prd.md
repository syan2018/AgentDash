# Agent steering 可视化与 Runtime Context Notice 结构化

## 背景

近期 capability / workflow runtime 重构已经把部分后端状态做成了结构化事件，但用户在前端仍无法稳定看到 Agent 实际被如何 steer：

- `capability_state_changed` 有结构化 payload，前端也会展示部分 diff，但 Agent 实际收到的是另一条 Markdown steering notice。
- `Runtime Tool Schema` 当前把完整工具 JSON schema 直接拼进 Markdown code block，用户侧无法标准化查看工具差异和当前可调用工具。
- Hook/context 注入仍是 `{ slot, source, content }` 扁平文本，前端统一显示为“已注入动态上下文”，掩盖 workflow guidance、tool schema、capability delta、system notice 等不同来源。

## 目标

- 新增 `RuntimeContextNotice` 作为 runtime steering 的唯一结构化源。
- 后端先构建结构化 notice，再由同一对象渲染 `agent_visible_text`，避免事件 JSON、Agent Markdown、前端摘要三份漂移。
- 会话事件流新增 `runtime_context_notice`，前端以该事件绘制 Agent 行为可视化卡片。
- 前端展示“摘要 + Agent 原文”：默认看结构化摘要，展开可核对 Agent 实际收到的完整文本。

## 结构契约

`RuntimeContextNotice` 字段：

- `id`
- `source`
- `phase_node`
- `apply_mode`
- `delivery_status`
- `agent_visible_text`
- `sections`
- `created_at_ms`

`sections` 支持：

- `capability_delta`：capability added/removed/effective、tool path blocked/unblocked/whitelisted、MCP/VFS diff。
- `tool_schema`：当前 provider request 生效的工具列表，含 name、description、parameters_schema、capability_key/source。
- `workflow_context`：active workflow step、guidance、context binding 摘要。
- `hook_injection`：普通 hook 注入，保留 slot/source/content，但带 title/summary。
- `system_notice`：turn-start system info queue / pending action 等系统级告知。

## 实施要点

1. 后端新增 runtime notice 类型与 renderer。
2. `HookTurnStartNotice` 保留 `content`，新增 `runtime_context_notice` 可选字段；`content` 必须由 notice 渲染而来。
3. runtime context transition 与 initial tool schema notice 都改为创建 `RuntimeContextNotice`，同时入队 turn-start notice。
4. session 事件流新增 `SessionMetaUpdate { key: "runtime_context_notice", value }`。
5. 前端新增 `RuntimeContextNoticeCard`，接入系统事件渲染。
6. 原 `CTX 已注入动态上下文` 仅作为无结构化 frame 的 legacy/debug 展示。

## 验收标准

- Plan → Apply 流转后，用户无需阅读 Agent 自述，也能看到 upsert 工具何时开放、当前完整可用工具、workflow guidance 如何注入。
- Agent 实际收到的文本与前端卡片展开内容一致。
- `capability_state_changed` 可保留为底层状态事件，但前端 steering 可视化以 `runtime_context_notice` 为准。
- 不新增数据库表，不做旧字段兼容；沿用现有 session event / platform event 持久化链路。

## 测试要求

- Rust 单测覆盖 notice 结构生成、同源渲染、tool schema section、事件持久化。
- 前端单测覆盖专用卡片渲染、diff 展示、tool schema 展开、Agent 原文展开。
- 至少运行 backend check 相关测试、frontend check。
