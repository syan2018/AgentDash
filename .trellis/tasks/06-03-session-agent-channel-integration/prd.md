# Session 与 Agent 会话信道整合

## Goal

把用户面向的 Session 页重新接回项目的 Agent 控制面主线：用户仍以 Session 作为一级心智与流式消息壳，但发送消息时必须进入 `LifecycleAgent -> AgentFrame -> LifecycleRun` 的统一行为入口，再由该入口投递到真实 `RuntimeSession` trace。

这项任务的用户价值是让 Project Agent 创建后的会话可以自然继续对话，不再在 Session 页输入后卡在 “Runtime trace 页面不再支持直接发送 prompt；请从 Run、Subject 或 Agent 入口派发执行。” 的断链状态。

## Confirmed Architecture Facts

- `LifecycleRun` 是业务执行控制账本，`LifecycleAgent` 是 run-scoped 执行身份，`AgentFrame` 是运行表面快照，`RuntimeSession` 是 connector delivery / trace evidence。
- Session 是用户可理解的消息流壳概念；业务归属与发送入口由 `RuntimeSessionExecutionAnchor`、`AgentFrame`、`LifecycleAgent`、`AgentAssignment` 和 `LifecycleSubjectAssociation` 反查。
- 前端 `/session/:id` 可以展示 runtime trace，但不能把 runtime trace 本身当成业务 command root。
- 当前前端 `SessionChatView` 在没有 `customSend` 时会抛出 Runtime trace 禁发提示；`SessionPage` 没有提供标准 Agent 发送入口，因此 Project Agent launch 后进入 Session 页会断链。
- 现有 Project Agent launch 可以创建 `LifecycleRun`、`LifecycleAgent`、`AgentFrame` 和 delivery `RuntimeSession`，但后续用户消息需要走同一个 Agent 控制面语义，而不是绕过 Agent 行为直接投递 runtime prompt。
- 存量 session-first 入口、注释和 spec 残留都不能当作权威路径；本任务以控制面架构不变量为准。

## Requirements

- 用户一级概念保持 Session；默认 UI 不新增 Lifecycle 作为用户心智入口。
- Project Agent launch 后跳转到 `/session/:runtime_session_id` 时，Session 页必须解析到对应 `LifecycleAgent` / `AgentFrame` 上下文，并具备继续发送用户消息的能力。
- Session 页发送消息的前端入口必须调用 Agent/Lifecycle 语义的服务函数；请求 payload 应携带后端能够定位 Agent 行为的 refs，例如 delivery runtime session、agent/frame/run refs、prompt blocks、executor config。
- 后端命令入口必须从 delivery runtime session 或显式 refs 解析到 `LifecycleAgent` / `AgentFrame`，再通过统一 Agent 行为路径派发；`RuntimeSession` 只作为投递目标与 trace ref。
- 发送成功后，前端应继续使用现有 Session stream/feed 展示用户消息、agent delta、tool card、terminal state 与错误状态。
- 若当前 Session 无法反查到 Agent/Frame，前端必须以明确的不可发送状态展示原因，并引导用户从 Agent 入口创建或重新打开会话。
- Session 页上的冗余运行态字段应优先从 AgentFrame/Lifecycle projection 派生；本任务只迁移发送链路所必需的状态，不做全面 Session 数据模型重构。
- 真实验证必须启动 `pnpm dev` 调试项目，并在浏览器中完成创建 Agent 会话、发送第一轮消息、等待响应、发送第二轮消息、等待响应的完整交互。

## Paths To Remove Or Seal

- 前端 `SessionChatView` 不能再呈现“无 Agent dispatcher 仍可发送”的普通输入体验；没有 `customSend` / Agent dispatcher 时应是明确不可发送状态，而不是发送后抛 Runtime trace 禁发错误。
- 前端不得新增或保留 `sendSessionPrompt(session_id, ...)`、`POST /sessions/{id}/prompt` 这类 session-first service；Session 页发送服务必须以 Agent/Lifecycle 命名和 refs 为入口。
- 后端不得把 `SessionRuntimeService::start_prompt` 暴露为用户 HTTP command root；它只能作为 LifecycleAgent/Task/Routine 等控制面 use case 内部的 runtime delivery 实现。
- spec 中把用户 prompt 表述为 `POST /sessions/{id}/prompt` 的陈旧路径必须更新为 LifecycleAgent command 语义，避免后续实现按旧图纸接回去。
- Runtime trace 页面可以保留 trace/fork/rollback/cancel/tool approval 等 trace 行为，但不能拥有普通用户消息发送的业务入口。

## Acceptance Criteria

- [x] Project Agent 入口可以创建/打开用户面向的 Session。
- [x] Session 页输入消息不会再触发 Runtime trace 禁发提示。
- [x] Session 页发送的消息经由 LifecycleAgent 统一入口解析 AgentFrame 并投递到对应 delivery RuntimeSession。
- [x] 前端可以连续完成至少两轮用户消息与 Agent 响应，消息流在 Session 页正确展示。
- [x] 无法解析 Agent/Frame 的 runtime trace 显示清晰的不可发送状态，不伪装成可发送会话。
- [x] session-first prompt 路径已被删除或封死，代码和 spec 不再暗示 `POST /sessions/{id}/prompt` 是用户消息入口。
- [x] `pnpm -C packages/app-web exec tsc --noEmit` 通过。
- [x] 相关前端/后端定向测试通过，覆盖 Session 页 custom send、不可发送状态、后端 Agent dispatch 入口和 session-first path guard。
- [x] `pnpm dev` 启动后通过真实浏览器交互验证创建 Agent 会话与两轮以上对话。

## Verification Result

- `pnpm dev` 本地调试栈已启动并验证 `http://127.0.0.1:5380/session/0150042e-84e7-4dc2-a855-e783107467a6`。
- 从 Agent Hub 的 Project Agent 入口创建 Session 后，连续发送 `codex verification with workspace turn one` 与 `codex verification with workspace turn two`，两次请求均为 `POST /api/lifecycle-agents/by-runtime-session/{runtime_session_id}/messages`，HTTP 200，Session 页展示用户消息、工具调用与 Agent 响应。
- 本地验证项目需要默认 Workspace 提供运行目录；已在调试数据中创建 `AgentDashboard local workspace` 并设为 Project 默认 Workspace。

## Out Of Scope

- 不在本任务中重做完整 Session 信息架构。
- 不把 Lifecycle 暴露成默认用户一级概念。
- 不做兼容旧 session-first command path 的回退方案。
